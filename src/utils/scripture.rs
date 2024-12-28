pub const SCRIPTURE_BASE_URL: &str = "https://scripture.egw.workers.dev";
pub const SCRIPTURE_BIBLE_REGEX: &str = r"(\d?\s?(?:[1-3]\s)?[a-z][a-z]+(?:\sof\s[a-z]+)?)\s?(\d+):(\d+)(?:-(\d+))?(?:\s?([a-z0-9]+))?";

use regex::RegexBuilder;
use serde::{Deserialize, Serialize};
use worker::{Error, Headers, RequestInit, Env, console_log, Fetch, Request};

#[derive(Debug, Serialize, Deserialize)]
pub struct ScriptureResponse {
    pub initial_reference: String,
    pub resolved_reference: String,
    pub translation: String,
    pub scripture: String,
    pub previous_verse: String,
    pub next_verse: String,
    pub content_type: String,
    pub partner_key: String,
}

impl ScriptureResponse {
    pub fn formatted(&self) -> String {
        format!("{} ({}) - {}", self.resolved_reference, self.translation, self.scripture)
    }
}

pub async fn get_scripture(reference: &str, translation: &str, env: &Env) -> worker::Result<String> {
    console_log!("Attempting to get scripture for reference: {}", reference);
    
    let scripture_regex = RegexBuilder::new(SCRIPTURE_BIBLE_REGEX)
        .case_insensitive(true)
        .unicode(true)
        .dot_matches_new_line(false)
        .build()
        .map_err(|e| Error::from(format!("Error compiling regex: {}", e)))?;

    if let Some(caps) = scripture_regex.captures(reference) {
        let (book, chapter, verse_start, verse_end, translation) = (
            caps.get(1).map_or("", |m| m.as_str().trim()),
            caps.get(2).map_or("", |m| m.as_str()),
            caps.get(3).map_or("", |m| m.as_str()),
            caps.get(4)
                .map_or_else(|| caps.get(3).map_or("", |m| m.as_str()), |m| m.as_str()),
            caps.get(5).map_or(translation, |m| m.as_str()),
        );

        console_log!("Parsed reference - Book: {}, Chapter: {}, Verse Start: {}, Verse End: {}, Translation: {}", 
            book, chapter, verse_start, verse_end, translation);

        let scripture_ref = if verse_start == verse_end {
            format!("{} {}:{}", book, chapter, verse_start)
        } else {
            format!("{} {}:{}-{}", book, chapter, verse_start, verse_end)
        };        

        match fetch_scripture_from_api(
            env,
            translation,
            &scripture_ref,
            "ChapterVerse",
            Some(500),
        )
        .await {
            Ok(scripture_response) => {
                console_log!("Received scripture response: {:?}", scripture_response.resolved_reference);
                Ok(scripture_response.formatted())
            }
            Err(e) => {
                console_log!("Error fetching scripture: {:?}", e);
                // During development, return a mock response
                #[cfg(debug_assertions)]
                {
                    Ok(format!("Development Mode - Reference: {} ({})", scripture_ref, translation))
                }
                #[cfg(not(debug_assertions))]
                {
                    Err(e)
                }
            }
        }
    } else {
        console_log!("No scripture reference match found in text: {}", reference);
        Err(Error::RustError("Invalid scripture reference".to_string()))
    }
}

pub async fn fetch_scripture_from_api(
    _env: &Env,
    translation: &str,
    reference: &str,
    partner: &str,
    length: Option<u16>,
) -> worker::Result<ScriptureResponse> {
    // URL encode the reference to handle spaces correctly
    let encoded_reference = reference.replace(' ', "%20");
    
    let mut url = format!(
        "{}/{}/{}?partner={}",
        SCRIPTURE_BASE_URL, translation, encoded_reference, partner
    );

    if let Some(len) = length {
        url.push_str(&format!("&length={}", len));
    }

    console_log!("Fetching scripture from URL: {}", url);

    let mut init = RequestInit::new();
    let mut headers = Headers::new();
    headers.append("Content-Type", "application/json")?;
    headers.append("Accept", "application/json")?;
    init.with_headers(headers);

    let mut response = Fetch::Request(Request::new_with_init(&url, &init)?).send().await?;
    let text = response.text().await?;
    // console_log!("Raw API response: {}", text);
    
    if text.is_empty() {
        return Err(Error::RustError("Empty response from scripture service".to_string()));
    }
    
    serde_json::from_str(&text)
        .map_err(|e| Error::from(format!("Failed to deserialize the scripture response: {}", e)))
} 