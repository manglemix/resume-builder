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
        // let scraper = state.get_scraper();
        // println!("{:?}", soup.tag("ul").class("ml-5 list-disc").find_all().map(|x| x.children().map(|x| x.text()).collect::<Vec<_>>()).collect::<Vec<_>>());
        todo!();
        
        Some(Ok(page_data))
    }
}