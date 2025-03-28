use serde::{Deserialize, Serialize};

/// It looks like all responses return 200 ok and then use the status field.
#[derive(Serialize, Deserialize, Debug)]
pub struct DisplayResponse {
    /// 0 looks to be a success
    pub status: u16,
    /// error message if there is one
    pub error: Option<String>,
    /// The bmp url
    pub image_url: Option<String>,
    /// Set from the plugin if not mistaken
    pub filename: Option<String>,
    /// How long till refresh in seconds I think?
    pub refresh_rate: Option<u64>,
    /// Seems to always be included
    pub reset_firmware: bool,
    pub update_firmware: Option<bool>,
    pub firmware_url: Option<String>,
    ///I think this is an enum so will swap over later when I learn more about the api
    pub special_function: Option<String>,
}
