use std::{sync::Arc, path::PathBuf};

use anyhow::Context;
use headless_chrome::{Tab, types::PrintToPdfOptions};
use ordered_float::OrderedFloat;
use regex::Regex;
use serde::Deserialize;
use tokio::fs::DirBuilder;
use validator::Validate;

use crate::page_scrapers::PageData;

const SMALLEST_FONT_PERCENTAGE: f64 = 0.0001;
const A4_PAGE_HEIGHT_PX: f64 = 973.0;
const DEFAULT_RESUME_HTML: &str = r#"
<!doctype html>
<meta charset="utf-8">
<name>
<hr>
<phonenumber><email><website>
<education>
    <h2>Education</h2>
    <hr>
    <entries>
</education>
<style>
    * {
        font-size: 1rem;
    }
</style>
"#;

/// Standardized information about some form of education, such as college/university.
#[derive(Deserialize, Validate)]
struct Education {
    /// Your final cumulative GPA, or the current cumulative GPA that you have.
    #[validate(range(min = 0))]
    gpa: f64,
    /// The maximum possible cumulative GPA that you can have.
    /// 
    /// If the maximum value is not 4, it is useful to let recruiters know.
    /// If it is unbounded, then you should use the expected GPA of well performing students.
    #[validate(range(min = 0))]
    max_gpa: Option<f64>,
    /// The year that you started as a student in this school.
    /// 
    /// Do not put schools that you expect to enroll in. Stick to schools that you
    /// have enrolled in or are enrolled in.
    #[validate(range(min = 1970, max = 2070))]
    start_year: u16,
    /// The month of the year that you started as a student in this school.
    #[validate(range(min = 1, max = 12))]
    start_month: u8,
    /// The year that you ended enrollment at this school, or the year that you expect
    /// to graduate.
    #[validate(range(min = 1970, max = 2070))]
    end_year: u16,
    /// The month of the year that you ended enrollment at this school, or the month of the year that you expect
    /// to graduate.
    #[validate(range(min = 1, max = 12))]
    end_month: u8,
    /// The name of this school.
    /// 
    /// Refrain from using acronyms.
    school_name: String,
    /// The major that is on your degree, or the major that you are currently pursuing
    major: String,
    #[serde(default)]
    /// Extra things that you would like recruiters.
    /// 
    /// Extra-curriculars that cannot stand in their own section can go here. Each
    /// entry will be scanned for keywords and inserted into a resume based on the
    /// keywords in a job posting.
    notes: Vec<String>
}

/// Information that the resume builder can use to create a concise and succint resume.
#[derive(Deserialize, Validate)]
pub(super) struct ResumeData {
    /// Your full name, as written on a government issued ID.
    name: String,
    /// A phone number that you can be readily contacted on.
    #[validate(phone)]
    phone_number: String,
    /// Your professional work email.
    /// 
    /// Debating on using your personal, potentially immature email or
    /// a professional email such as first_name.last_name@gmail.com? Save
    /// yourself the cognitive dissonance and get a professional email before
    /// someone reserves it!
    /// 
    /// Have your own domain that you want to register your email with? Make sure
    /// that you can rely on it. The last thing you want is to miss emails from recruiters
    /// because you forgot to check or pay for this email.
    #[validate(email)]
    email: String,
    /// A website where people can learn about who you are and what you do.
    /// 
    /// If you do not have a website, do not put your linkedin here. There is
    /// a dedicated field for linkedin
    #[validate(url)]
    website: Option<String>,
    /// A link to your linkedin.
    /// 
    /// Consider simplifying your public address if possible. This field is not optional since
    /// so many job application sites ask for it, and a linkedin account is free.
    #[validate(url)]
    linkedin: String,
    /// Your permanent address.
    /// 
    /// Nowadays, you may not need a permanent address in your resume since you can be contacted
    /// through email, phone, or linkedin, instead of by mail. If a company really needed your address,
    /// they may ask you directly towards the end of the recruitment process.
    address: Option<String>,
    /// A collection of information regarding schools you've attended.
    education: Vec<Education>
}

pub(super) const OUTPUT_PATH: &str = "resumes/";


pub(super) async fn use_page_data(page_data: PageData, mut tab: Arc<Tab>, mut resume_data: Arc<ResumeData>, mut resume_template: Option<Arc<String>>) -> anyhow::Result<()> {
    let selected_template = resume_template.as_ref().map(|x| x.as_str()).unwrap_or(DEFAULT_RESUME_HTML);
    let font_size_regex = Regex::new(r#"font-size:(.|\n)*\d+\.*\d*.*;"#).unwrap();
    let min_font_size = font_size_regex
        .find_iter(selected_template)
        .filter_map(|x| {
            let line = x.as_str();
            let number_unit_str = line.split_at(10).1.trim();
            let number_str;
            let multiplier;
            if number_unit_str.ends_with("rem;") {
                number_str = number_unit_str.split_at(number_unit_str.len() - 4).0.trim();
                multiplier = 16.0;
            } else if number_unit_str.ends_with("px;") {
                number_str = number_unit_str.split_at(number_unit_str.len() - 3).0.trim();
                multiplier = 1.0;
            } else {
                return None
            }
            let size: Option<f64> = number_str.parse().ok();
            size.map(|x| OrderedFloat(x * multiplier))
        })
        .min()
        .map(|x| x.0)
        .unwrap_or(16.0);
    let mut page_scale = 1.0;
    let mut too_many_lines = false;
    
    let resume_bytes = loop {
        let (resume_body, tmp1, tmp2) = tokio_rayon::spawn(move || {
            let resume_body = resume_template.as_ref().map(|x| x.as_str()).unwrap_or(DEFAULT_RESUME_HTML);
    
            macro_rules! sub {
                ($body: expr, $tag: literal, $($arg:tt)*) => {
                    Regex::new($tag).unwrap().replace_all(&$body, |_: &regex::Captures| $($arg)*)
                };
            }
    
            let resume_body = sub!(resume_body, "<phonenumber>", format!("{}", resume_data.phone_number));
            let resume_body = sub!(resume_body, "<email>", format!("|<a href=mailto:{}>Email</a>", resume_data.email));
            let resume_body = match resume_data.website.as_ref() {
                Some(website) => sub!(resume_body, "<website>", format!("|<a href={}>Website</a>", website)),
                None => resume_body
            };
    
            (resume_body.into_owned(), resume_data, resume_template)
        }).await;

        resume_data = tmp1;
        resume_template = tmp2;
    
        let (height, tmp) = tokio_rayon::spawn::<_, anyhow::Result<_>>(move || {
            tab.navigate_to(&format!("data:text/html,{resume_body}"))?;
            Ok((tab.find_element("html").unwrap().get_box_model().unwrap().height, tab))
        }).await?;

        tab = tmp;
        if height > A4_PAGE_HEIGHT_PX {
            page_scale = A4_PAGE_HEIGHT_PX / height;
        }
        if min_font_size / height < SMALLEST_FONT_PERCENTAGE {
            page_scale *= SMALLEST_FONT_PERCENTAGE / min_font_size * height;
            too_many_lines = true;
            continue;
        }

        break tokio_rayon::spawn(move || {
            tab.print_to_pdf(Some(PrintToPdfOptions {
                scale: Some(page_scale),
                ..Default::default()
            }))
        }).await?;
    };

    let folder_path = PathBuf::from(OUTPUT_PATH).join(format!("{} {}", page_data.company, page_data.job_title));
    DirBuilder::new().recursive(true).create(&folder_path).await.context("Failed to create a directory in resumes. Do we have permissions?")?;
    tokio::fs::write(folder_path.join("resume.pdf"), resume_bytes).await?;
    Ok(())
}