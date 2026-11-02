use std::io::{self, Read, Seek, SeekFrom};
use std::sync::Mutex;
use std::time::Duration;

use ureq::Agent;

pub fn fetch_agent() -> Agent {
    Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(12)))
        .timeout_recv_response(Some(Duration::from_secs(20)))
        .build()
        .into()
}

pub fn stream_agent() -> Agent {
    Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(12)))
        .timeout_recv_response(Some(Duration::from_secs(20)))
        .build()
        .into()
}

struct StreamState {
    reader: Option<Box<dyn Read + Send>>,
    resp_pos: u64,
}

pub struct HttpStream {
    agent: Agent,
    url: String,
    headers: Vec<(String, String)>,
    len: Option<u64>,
    pos: u64,
    state: Mutex<StreamState>,
}

impl HttpStream {
    pub fn open(
        agent: Agent,
        url: String,
        headers: Vec<(String, String)>,
    ) -> Result<Self, String> {
        let mut stream = Self {
            agent,
            url,
            headers,
            len: None,
            pos: 0,
            state: Mutex::new(StreamState {
                reader: None,
                resp_pos: 0,
            }),
        };
        stream.reopen(0).map_err(|err| err.to_string())?;
        Ok(stream)
    }

    fn reopen(&mut self, at: u64) -> io::Result<()> {
        let mut request = self
            .agent
            .get(&self.url)
            .header("Range", &format!("bytes={at}-"));
        for (key, value) in &self.headers {
            request = request.header(key, value);
        }

        let response = request.call().map_err(|err| {
            io::Error::new(io::ErrorKind::ConnectionRefused, err.to_string())
        })?;
        let status = response.status().as_u16();
        let content_length = response
            .headers()
            .get("content-length")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok());
        let total = response
            .headers()
            .get("content-range")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split('/').nth(1))
            .and_then(|value| value.parse::<u64>().ok());

        self.len = match (status, total, content_length) {
            (_, Some(total), _) => Some(total),
            (200, None, Some(length)) => Some(length),
            _ => self.len,
        };

        let mut reader: Box<dyn Read + Send> = Box::new(response.into_body().into_reader());

        if status != 206 && at > 0 {
            if at > 8 * 1024 * 1024 {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "server does not support seeking this stream",
                ));
            }
            let mut discarded = 0u64;
            while discarded < at {
                let mut scratch = [0u8; 8192];
                let read = reader.read(&mut scratch)?;
                if read == 0 {
                    break;
                }
                discarded += read as u64;
            }
        }

        let mut state = self.lock();
        state.reader = Some(reader);
        state.resp_pos = at;
        Ok(())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, StreamState> {
        self.state.lock().unwrap_or_else(|err| err.into_inner())
    }

    fn read_once(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let needs_reopen = {
            let state = self.lock();
            state.reader.is_none() || state.resp_pos != self.pos
        };
        if needs_reopen {
            let at = self.pos;
            self.reopen(at)?;
        }

        let read = {
            let mut state = self.lock();
            let Some(reader) = state.reader.as_mut() else {
                return Ok(0);
            };
            let read = reader.read(buf)?;
            if read == 0 {
                state.reader = None;
                return Ok(0);
            }
            state.resp_pos += read as u64;
            read
        };
        self.pos += read as u64;
        Ok(read)
    }
}

impl Read for HttpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if let Some(len) = self.len {
            if self.pos >= len {
                return Ok(0);
            }
        }

        let read = self.read_once(buf)?;
        if read == 0 {
            if let Some(len) = self.len {
                if self.pos >= len {
                    return Ok(0);
                }
            }
            return self.read_once(buf);
        }
        Ok(read)
    }
}

impl Seek for HttpStream {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let current = self.pos as i64;
        let target = match position {
            SeekFrom::Start(offset) => offset as i64,
            SeekFrom::End(offset) => self.len.unwrap_or(0) as i64 + offset,
            SeekFrom::Current(offset) => current + offset,
        };
        if target < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek before start of stream",
            ));
        }
        let target = target as u64;
        if let Some(len) = self.len {
            if target > len {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "seek past end of stream",
                ));
            }
        }
        if target != self.pos {
            self.pos = target;
            let mut state = self.lock();
            state.reader = None;
        }
        Ok(target)
    }
}
