use super::*;
use cap_std::fs::Dir;
use std::{io::Read, time::Duration};
#[cfg(test)]
mod tests;

/// Explicit distribution authority; metadata never chooses a remote endpoint.
pub struct Source {
    inner: Transport,
}
enum Transport {
    Local(Dir),
    Https { base: String, agent: ureq::Agent },
}
impl Source {
    pub fn local(path: &Path) -> Result<Self, Error> {
        let dir = Dir::open_ambient_dir(path, cap_std::ambient_authority())
            .map_err(|e| io("open local distribution", path, e))?;
        Ok(Self {
            inner: Transport::Local(dir),
        })
    }
    pub fn https(base: &str) -> Result<Self, Error> {
        Self::with_agent(base, agent())
    }
    fn with_agent(base: &str, agent: ureq::Agent) -> Result<Self, Error> {
        let uri: ureq::http::Uri = base
            .parse()
            .map_err(|_| state("invalid HTTPS distribution base"))?;
        if base.len() > 2048
            || base.contains(['@', '?', '#', '\\'])
            || uri.scheme_str() != Some("https")
            || uri.host().is_none_or(str::is_empty)
        {
            return Err(state(
                "HTTPS base requires a host and no credentials, query or fragment",
            ));
        }
        Ok(Self {
            inner: Transport::Https {
                base: base.trim_end_matches('/').into(),
                agent,
            },
        })
    }
    pub(super) fn reader(
        &self,
        relative: &str,
        exact: Option<u64>,
    ) -> Result<Box<dyn Read>, Error> {
        match &self.inner {
            Transport::Local(dir) => {
                let file = storage::open_file(dir, Path::new(relative), false)?;
                if exact
                    .is_some_and(|size| file.metadata().map(|m| m.len() != size).unwrap_or(true))
                {
                    return Err(state("payload declared length mismatch"));
                }
                Ok(Box::new(file))
            }
            Transport::Https { base, agent } => {
                let response = agent
                    .get(format!("{base}/{relative}"))
                    .header("Accept-Encoding", "identity")
                    .call()
                    .map_err(|error| Error {
                        failure: Failure::Transport(Box::new(error)),
                        floor_advanced: false,
                    })?;
                if response.status().as_u16() != 200 {
                    return Err(state(
                        "HTTPS distribution requires status 200 without redirects",
                    ));
                }
                for encoding in response.headers().get_all("content-encoding") {
                    if encoding.as_bytes() != b"identity" {
                        return Err(state("encoded distribution response refused"));
                    }
                }
                if let (Some(size), Some(length)) =
                    (exact, response.headers().get("content-length"))
                    && length.to_str().ok().and_then(|s| s.parse::<u64>().ok()) != Some(size)
                {
                    return Err(state("HTTP payload declared length mismatch"));
                }
                Ok(Box::new(response.into_body().into_reader()))
            }
        }
    }
    pub(super) fn metadata(&self, channel: Channel, target: Target) -> Result<Vec<u8>, Error> {
        let path = format!("channels/{}/{}.json", channel.text(), target.text());
        let mut bytes = Vec::new();
        self.reader(&path, None)?
            .take(MAX_METADATA_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| io("read distribution metadata", &path, e))?;
        if bytes.len() > MAX_METADATA_BYTES {
            return Err(state("distribution metadata byte limit exceeded"));
        }
        Ok(bytes)
    }
}
fn agent() -> ureq::Agent {
    configured_agent(ureq::tls::TlsConfig::default(), Duration::from_secs(120))
}
fn configured_agent(tls: ureq::tls::TlsConfig, total: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .https_only(true)
        .max_redirects(0)
        .proxy(None)
        .max_response_header_size(64 * 1024)
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_global(Some(total))
        .tls_config(tls)
        .build()
        .new_agent()
}
