use scraper::Selector;

use super::{PageScraper, PageData, ScraperState};

/// A scraper for MyWorkday job sites
#[derive(Default)]
pub(super) struct WorkdayScraper;

impl PageScraper for WorkdayScraper {
    const NAME: &'static str = "workday";

    fn scrape(state: &ScraperState) -> Option<anyhow::Result<PageData>> {
        if !state.url.host_str().unwrap().contains("myworkdaysite.com") {
            return None;
        }
        let mut page_data = state.create_page_data();
        page_data.company = state.url.path_segments().expect("Job application website should have been valid").take(2).last()?.to_string();
        let scraper = state.get_scraper();

        page_data.job_title = scraper
            .select(&Selector::parse("h2[data-automation-id=\"jobPostingHeader\"]").unwrap())
            .next()
            .map(|x| x.text().map(|x| x.replace("\u{a0}", " ")).collect())?;

        let job_posting_desc = scraper
            .select(&Selector::parse("div[data-automation-id=\"jobPostingDescription\"]").unwrap())
            .next()?;
        
        let lines = job_posting_desc
            .select(&Selector::parse("li").unwrap())
            .map(|x| x.text().map(|x| x.replace("\u{a0}", " ")).collect())
            .collect();

        state
            .extract_keywords(lines)
            .get()
            .into_iter()
            .map(|x| x.into_iter())
            .flatten()
            .for_each(|x| {
                let k = super::KeyWithData { key: x.text, data: x.score };
                if let Some(mut old_k) = page_data.keywords.take(&k) {
                    old_k.data += k.data;
                    page_data.keywords.insert(old_k);
                } else {
                    page_data.keywords.insert(k);
                }
            });
        
        Some(Ok(page_data))
    }
}