use std::collections::{HashMap, HashSet};
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use ureq::Agent;

const CHUNK: u64 = 1024 * 1024;

pub fn range_header(at: u64, len: Option<u64>) -> String {
    let mut end = at + CHUNK - 1;
    if let Some(len) = len {
        end = end.min(len.saturating_sub(1));
    }
    format!("bytes={at}-{end}")
}

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

const MAX_CHUNKS: usize = 40;

const MAX_WHOLE_BODY: u64 = 128 * 1024 * 1024;

#[derive(Default)]
struct Cache {
    chunks: HashMap<u64, Arc<Vec<u8>>>,
    inflight: HashSet<u64>,
    len: Option<u64>,
}

struct Shared {
    cache: Mutex<Cache>,
    ready: Condvar,
}

pub struct HttpStream {
    agent: Agent,
    url: String,
    headers: Vec<(String, String)>,
    pos: u64,
    shared: Arc<Shared>,
}

type Fetched = (Vec<(u64, Vec<u8>)>, Option<u64>);

fn fetch_chunk(
    agent: &Agent,
    url: &str,
    headers: &[(String, String)],
    index: u64,
    known_len: Option<u64>,
) -> io::Result<Fetched> {
    let at = index * CHUNK;
    let mut last = io::Error::other("no attempt made");
    for attempt in 0..3u64 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(250 * attempt));
        }
        let mut request = agent.get(url).header("Range", &range_header(at, known_len));
        for (key, value) in headers {
            request = request.header(key, value);
        }
        let mut response = match request.call() {
            Ok(response) => response,
            Err(err) => {
                last = io::Error::new(io::ErrorKind::ConnectionRefused, err.to_string());
                continue;
            }
        };
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
        let mut body = Vec::new();
        let limit = if status == 206 { CHUNK } else { MAX_WHOLE_BODY };
        if let Err(err) = response.body_mut().as_reader().take(limit).read_to_end(&mut body) {
            last = err;
            continue;
        }
        if status == 206 {
            return Ok((vec![(index, body)], total.or(known_len)));
        }
        let whole = body.len() as u64;
        let pieces = body
            .chunks(CHUNK as usize)
            .enumerate()
            .map(|(n, piece)| (n as u64, piece.to_vec()))
            .collect();
        return Ok((pieces, content_length.or(Some(whole))));
    }
    Err(last)
}

fn load_chunk(shared: &Arc<Shared>, agent: &Agent, url: &str, headers: &[(String, String)], index: u64) -> io::Result<()> {
    {
        let mut cache = shared.cache.lock().unwrap_or_else(|err| err.into_inner());
        if cache.chunks.contains_key(&index) {
            return Ok(());
        }
        while cache.inflight.contains(&index) {
            cache = shared.ready.wait(cache).unwrap_or_else(|err| err.into_inner());
            if cache.chunks.contains_key(&index) {
                return Ok(());
            }
        }
        cache.inflight.insert(index);
    }
    let known_len = shared.cache.lock().unwrap_or_else(|err| err.into_inner()).len;
    let result = fetch_chunk(agent, url, headers, index, known_len);
    let mut cache = shared.cache.lock().unwrap_or_else(|err| err.into_inner());
    cache.inflight.remove(&index);
    let outcome = match result {
        Ok((pieces, total)) => {
            if total.is_some() {
                cache.len = total;
            }
            for (piece_index, data) in pieces {
                cache.chunks.insert(piece_index, Arc::new(data));
            }
            Ok(())
        }
        Err(err) => Err(err),
    };
    if cache.chunks.len() > MAX_CHUNKS {
        let keep = index;
        let mut indexes: Vec<u64> = cache.chunks.keys().copied().collect();
        indexes.sort_by_key(|candidate| std::cmp::Reverse(candidate.abs_diff(keep)));
        for victim in indexes.into_iter().take(cache.chunks.len() - MAX_CHUNKS) {
            cache.chunks.remove(&victim);
        }
    }
    shared.ready.notify_all();
    outcome
}

impl HttpStream {
    pub fn open(
        agent: Agent,
        url: String,
        headers: Vec<(String, String)>,
    ) -> Result<Self, String> {
        let shared = Arc::new(Shared {
            cache: Mutex::new(Cache::default()),
            ready: Condvar::new(),
        });
        load_chunk(&shared, &agent, &url, &headers, 0).map_err(|err| err.to_string())?;
        let stream = Self { agent, url, headers, pos: 0, shared };
        if let Some(len) = stream.len() {
            let count = len.div_ceil(CHUNK);
            for index in 1..count {
                if count <= 12 || index < 4 || index + 2 >= count {
                    stream.prefetch(index);
                }
            }
        }
        Ok(stream)
    }

    pub fn len(&self) -> Option<u64> {
        self.shared.cache.lock().unwrap_or_else(|err| err.into_inner()).len
    }

    fn prefetch(&self, index: u64) {
        let len = self.len();
        if len.is_some_and(|len| index * CHUNK >= len) {
            return;
        }
        {
            let cache = self.shared.cache.lock().unwrap_or_else(|err| err.into_inner());
            if cache.chunks.contains_key(&index) || cache.inflight.contains(&index) {
                return;
            }
        }
        let shared = self.shared.clone();
        let agent = self.agent.clone();
        let url = self.url.clone();
        let headers = self.headers.clone();
        let _ = std::thread::Builder::new().name("aria-prefetch".into()).spawn(move || {
            let _ = load_chunk(&shared, &agent, &url, &headers, index);
        });
    }
}

impl Read for HttpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.len().is_some_and(|len| self.pos >= len) {
            return Ok(0);
        }
        let index = self.pos / CHUNK;
        load_chunk(&self.shared, &self.agent, &self.url, &self.headers, index)?;
        let chunk = {
            let cache = self.shared.cache.lock().unwrap_or_else(|err| err.into_inner());
            cache.chunks.get(&index).cloned()
        };
        let Some(chunk) = chunk else {
            return Err(io::Error::other("chunk was evicted"));
        };
        let offset = (self.pos - index * CHUNK) as usize;
        if offset >= chunk.len() {
            return Ok(0);
        }
        let count = buf.len().min(chunk.len() - offset);
        buf[..count].copy_from_slice(&chunk[offset..offset + count]);
        self.pos += count as u64;
        if offset + count > chunk.len() / 4 {
            self.prefetch(index + 1);
        }
        Ok(count)
    }
}

impl Seek for HttpStream {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let current = self.pos as i64;
        let target = match position {
            SeekFrom::Start(offset) => offset as i64,
            SeekFrom::End(offset) => self.len().unwrap_or(0) as i64 + offset,
            SeekFrom::Current(offset) => current + offset,
        };
        if target < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek before start of stream",
            ));
        }
        let target = target as u64;
        if self.len().is_some_and(|len| target > len) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek past end of stream",
            ));
        }
        self.pos = target;
        Ok(target)
    }
}

#[cfg(test)]
mod tests {
    use super::{range_header, HttpStream};
    use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
    use std::net::TcpListener;

    fn serve(data: Vec<u8>, honor_range: bool) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("addr");
        std::thread::spawn(move || {
            while let Ok((mut stream, _)) = listener.accept() {
                let data = data.clone();
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                    let mut range: Option<(usize, usize)> = None;
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                            break;
                        }
                        if let Some(rest) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
                            let mut parts = rest.trim().split('-');
                            let start = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
                            let end = parts.next().and_then(|v| v.parse().ok()).unwrap_or(data.len() - 1);
                            range = Some((start, end.min(data.len() - 1)));
                        }
                    }
                    let (head, body) = match range {
                        Some((start, end)) if honor_range => (
                            format!(
                                "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{end}/{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                data.len(),
                                end - start + 1
                            ),
                            data[start..=end].to_vec(),
                        ),
                        _ => (
                            format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", data.len()),
                            data.clone(),
                        ),
                    };
                    let _ = stream.write_all(head.as_bytes());
                    let _ = stream.write_all(&body);
                });
            }
        });
        format!("http://{address}/a.m4a")
    }

    fn sample(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 253) as u8).collect()
    }

    #[test]
    fn reads_across_chunks_and_seeks_both_ways() {
        let data = sample(2_500_000);
        let mut stream = HttpStream::open(super::stream_agent(), serve(data.clone(), true), Vec::new()).expect("open");
        assert_eq!(stream.len(), Some(2_500_000));
        let mut all = Vec::new();
        stream.read_to_end(&mut all).expect("read");
        assert_eq!(all, data);
        stream.seek(SeekFrom::End(-100)).unwrap();
        let mut tail = Vec::new();
        stream.read_to_end(&mut tail).unwrap();
        assert_eq!(tail, &data[2_499_900..]);
        stream.seek(SeekFrom::Start(1_048_570)).unwrap();
        let mut across = [0u8; 20];
        stream.read_exact(&mut across).unwrap();
        assert_eq!(&across[..], &data[1_048_570..1_048_590]);
        assert!(stream.seek(SeekFrom::Start(3_000_000)).is_err());
    }

    #[test]
    fn works_when_the_server_ignores_range() {
        let data = sample(1_300_000);
        let mut stream = HttpStream::open(super::stream_agent(), serve(data.clone(), false), Vec::new()).expect("open");
        assert_eq!(stream.len(), Some(1_300_000));
        let mut all = Vec::new();
        stream.read_to_end(&mut all).expect("read");
        assert_eq!(all, data);
    }

    #[test]
    fn requests_are_bounded_chunks() {
        assert_eq!(range_header(0, None), "bytes=0-1048575");
        assert_eq!(range_header(1_048_576, Some(4_323_485)), "bytes=1048576-2097151");
        assert_eq!(range_header(4_000_000, Some(4_323_485)), "bytes=4000000-4323484");
    }
}
