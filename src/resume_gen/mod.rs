use std::{sync::Arc, path::PathBuf};

use anyhow::Context;
use headless_chrome::{Tab, types::PrintToPdfOptions};
use regex::{Regex, Captures};
use serde::Deserialize;
use tokio::fs::DirBuilder;
use validator::Validate;

use crate::page_scrapers::PageData;

pub(super) const SMALLEST_FONT_PERCENTAGE: f64 = 0.0003;
pub(super) const A4_PAGE_HEIGHT_PX: f64 = 973.0;
const DEFAULT_RESUME_HTML: &str = include_str!("default_template.html");
const MIN_DEFAULT_RESUME_FONT_SIZE: f64 = 16.0;


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


#[derive(Clone)]
pub(super) enum ResumeTemplate {
    Custom {
        template: Arc<String>,
        min_font_size: f64
    },
    Default
}


pub(super) struct Regexes {
    name: Regex,
    phonenumber: Regex,
    email: Regex,
    website: Regex,
    education: Regex,
    education_entries: Regex,
    school_name: Regex,
    gpa: Regex,
    max_gpa: Regex
}


impl Default for Regexes {
    fn default() -> Self {
        Self {
            name: Regex::new("<name>").unwrap(),
            phonenumber: Regex::new("<phonenumber>").unwrap(),
            email: Regex::new("<email>").unwrap(),
            website: Regex::new("<website>").unwrap(),
            education: Regex::new("<education>(.|\n)*</education>").unwrap(),
            education_entries: Regex::new("<entries>(.|\n)*</entries>").unwrap(),
            school_name: Regex::new("<school-name>").unwrap(),
            gpa: Regex::new("<gpa>").unwrap(),
            max_gpa: Regex::new("<max-gpa>").unwrap(),
        }
    }
}


pub(super) async fn use_page_data(page_data: PageData, tab: Arc<Tab>, resume_data: Arc<ResumeData>, resume_template: ResumeTemplate, regexes: Arc<Regexes>) -> anyhow::Result<()> {
    let resume_bytes = tokio_rayon::spawn(move || {
        let mut page_scale = 1.0;
        let mut too_many_lines = false;
        let (resume_body, min_font_size) = if let ResumeTemplate::Custom { template, min_font_size } = &resume_template {
            (template.as_str(), *min_font_size)
        } else {
            (DEFAULT_RESUME_HTML, MIN_DEFAULT_RESUME_FONT_SIZE)
        };

        loop {
            macro_rules! sub {
                ($body: expr, $reg: ident, $($arg:tt)*) => {
                    regexes.$reg.replace_all(&$body, $($arg)*)
                };
            }
    
            let resume_body = sub!(resume_body, name, |_: &Captures| format!("<div class=\"name\">{}</div>", resume_data.name));
            let resume_body = sub!(resume_body, phonenumber, |_: &Captures| format!("<div class=\"phonenumber\">{}</div>", resume_data.phone_number));
            let resume_body = sub!(resume_body, email, |_: &Captures| format!("<a class=\"email\" href=mailto:{}>Email</a>", resume_data.email));
            let resume_body = match resume_data.website.as_ref() {
                Some(website) => sub!(resume_body, website, |_: &Captures| format!("<a class=\"website\" href={website}>Website</a>")),
                None => resume_body
            };
            let resume_body = sub!(resume_body, education, |c: &Captures| {
                let matched = c.get(0).unwrap().as_str();
                // Remove <education> tags
                let education_block = matched.split_at(matched.len() - 12).0.split_at(11).1;
                regexes
                    .education_entries
                    .replace_all(education_block, |c: &Captures| {
                        let matched = c.get(0).unwrap().as_str();
                        // Remove <entries> tags
                        let entry = matched.split_at(matched.len() - 10).0.split_at(9).1;

                        resume_data.education
                            .iter()
                            .map(|education| {
                                let entry = sub!(entry, school_name, |_: &Captures| format!("<div class=\"school-name\">{}</div>", education.school_name));
                                let entry = sub!(entry, gpa, |_: &Captures| format!("<div class=\"gpa\">{}</div>", education.gpa));
                                let entry = match education.max_gpa {
                                    Some(x) => sub!(entry, max_gpa, |_: &Captures| format!("<div class=\"max-gpa\">/{x}</div>")),
                                    None => entry
                                };
                                entry.into_owned()
                            })
                            .collect::<String>()
                    }).into_owned()
            });

            tab
                .navigate_to(&format!("data:text/html,{resume_body}"))?
                .wait_until_navigated()?;
            let mut height = tab.find_element("html").unwrap().get_box_model().unwrap().height;
    
            if height > A4_PAGE_HEIGHT_PX {
                let new_page_scale = A4_PAGE_HEIGHT_PX / height;
                height *= new_page_scale / page_scale;
                page_scale = new_page_scale;
            }
            if min_font_size / height < SMALLEST_FONT_PERCENTAGE {
                page_scale *= SMALLEST_FONT_PERCENTAGE / min_font_size * height;
                too_many_lines = true;
                continue;
            }
            if page_scale < 1.0 {
                page_scale = 1.0f64.min(A4_PAGE_HEIGHT_PX / height * page_scale);
            }
    
            break tab.print_to_pdf(Some(PrintToPdfOptions {
                scale: Some(page_scale),
                ..Default::default()
            }))
        }
    }).await?;

    let folder_path = PathBuf::from(OUTPUT_PATH).join(format!("{} {}", page_data.company, page_data.job_title));
    DirBuilder::new().recursive(true).create(&folder_path).await.context("Failed to create a directory in resumes. Do we have permissions?")?;
    tokio::fs::write(folder_path.join("resume.pdf"), resume_bytes).await?;
    Ok(())
}