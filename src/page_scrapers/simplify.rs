use super::{PageScraper, PageData};

/// A scraper for Simplify job sites
#[derive(Default)]
pub(crate) struct SimplifyScraper;

impl PageScraper for SimplifyScraper {
    fn scrape(html: &str, url: &url::Url) -> Option<anyhow::Result<PageData>> {
        todo!()
    }
}