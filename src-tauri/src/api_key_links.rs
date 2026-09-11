use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct ApiKeyPageRequest {
    pub resource: String,
}

fn resource_url(resource: &str) -> Result<String, String> {
    // Only fixed official pages bundled with the app can reach the OS opener.
    // Never accept a URL, workspace path or credential from the webview.
    let resources: serde_json::Value =
        serde_json::from_str(include_str!("../../src/providerKeyResources.json"))
            .map_err(|_| "API key help is unavailable".to_string())?;
    resources
        .get(resource)
        .and_then(|entry| entry.get("url"))
        .and_then(serde_json::Value::as_str)
        .filter(|url| url.starts_with("https://"))
        .map(str::to_owned)
        .ok_or_else(|| "Unknown API key resource".to_string())
}

pub(crate) fn open(request: ApiKeyPageRequest) -> Result<(), String> {
    let url = resource_url(&request.resource)?;
    #[cfg(target_os = "windows")]
    let mut command = std::process::Command::new("explorer.exe");
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let mut command = std::process::Command::new("xdg-open");
    command
        .arg(url)
        .spawn()
        .map_err(|_| "Unable to open browser".to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_bundled_official_pages_can_be_opened() {
        for resource in ["gemini", "openai", "grok", "mistral", "deepseek"] {
            let url = resource_url(resource).unwrap();
            assert!(url.starts_with("https://"));
            assert!(!url.contains("token="));
        }
        for invalid in [
            "https://example.com",
            "file:///private",
            "../LICENSE",
            "cmd.exe",
            "",
        ] {
            assert!(resource_url(invalid).is_err());
        }
    }
}
