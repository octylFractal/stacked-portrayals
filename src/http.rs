use error_stack::{Report, ResultExt};
use reqwest::blocking::Response;
use serde::Deserialize;

use crate::SPError;

pub fn good_error_request_json<T: for<'de> Deserialize<'de>>(
    url: &str,
) -> Result<T, Report<SPError>> {
    good_error_request(url)?
        .json()
        .change_context(SPError)
        .attach_printable_lazy(|| format!("Failed to parse JSON from {}", url))
}

pub fn good_error_request(url: &str) -> Result<Response, Report<SPError>> {
    reqwest::blocking::get(url)
        .and_then(|r| r.error_for_status())
        .change_context(SPError)
        .attach_printable_lazy(|| format!("Failed to make request to {}", url))
}
