use super::{PageScraper, PageData, ScraperState};

/// A scraper for Simplify job sites
#[derive(Default)]
pub(super) struct SimplifyScraper;

impl PageScraper for SimplifyScraper {
    const NAME: &'static str = "simplify";
    
    fn scrape(state: &ScraperState) -> Option<anyhow::Result<PageData>> {
        if !state.url.host_str().unwrap().contains("simplify.jobs") {
            return None;
        }
        let page_data = state.create_page_data();
        let scraper = state.get_scraper();
        
        return None;
        
        Some(Ok(page_data))
    }
}