//! HTTPS GET sederhana lewat WinHTTP bawaan Windows (TLS dari sistem,
//! tidak menambah ukuran exe). Hanya HTTPS, dengan timeout dan batas ukuran.

use std::ffi::c_void;

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Networking::WinHttp::*;

const MAX_BYTES: usize = 512 * 1024;
const TIMEOUT_MS: i32 = 15_000;
const USER_AGENT: &str = "PatPet/2.0 (Windows desktop widget)";

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// Tidak ada koneksi, timeout, atau respons rusak/terlalu besar.
    Network,
    /// Server menjawab dengan status selain 200.
    Status(u32),
}

/// Handle WinHTTP yang ditutup otomatis.
struct Handle(*mut c_void);

impl Handle {
    fn new(raw: *mut c_void) -> Result<Self, Error> {
        if raw.is_null() { Err(Error::Network) } else { Ok(Self(raw)) }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: handle dibuat WinHTTP dan ditutup sekali di sini.
        unsafe {
            let _ = WinHttpCloseHandle(self.0);
        }
    }
}

/// GET `https://{host}{path}` dan kembalikan isinya sebagai teks.
pub fn get(host: &str, path: &str) -> Result<String, Error> {
    let net = |_| Error::Network;
    let agent = HSTRING::from(USER_AGENT);
    let host = HSTRING::from(host);
    let path = HSTRING::from(path);
    let language: Vec<u16> = "Accept-Language: id\r\n".encode_utf16().collect();

    // SAFETY: setiap handle dibungkus `Handle` sehingga ditutup urut terbalik;
    // buffer baca milik kita dan ukurannya dikirim apa adanya.
    unsafe {
        let session = Handle::new(WinHttpOpen(
            &agent,
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        ))?;
        WinHttpSetTimeouts(session.0, TIMEOUT_MS, TIMEOUT_MS, TIMEOUT_MS, TIMEOUT_MS).map_err(net)?;

        let connect = Handle::new(WinHttpConnect(session.0, &host, INTERNET_DEFAULT_HTTPS_PORT, 0))?;
        let request = Handle::new(WinHttpOpenRequest(
            connect.0,
            &HSTRING::from("GET"),
            &path,
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ))?;

        WinHttpSendRequest(request.0, Some(&language), None, 0, 0, 0).map_err(net)?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(net)?;

        let mut status = 0u32;
        let mut len = size_of::<u32>() as u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some(&mut status as *mut u32 as *mut c_void),
            &mut len,
            std::ptr::null_mut(),
        )
        .map_err(net)?;
        if status != 200 {
            return Err(Error::Status(status));
        }

        let mut body = Vec::new();
        let mut chunk = [0u8; 8 * 1024];
        loop {
            let mut read = 0u32;
            WinHttpReadData(request.0, chunk.as_mut_ptr().cast(), chunk.len() as u32, &mut read).map_err(net)?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..read as usize]);
            if body.len() > MAX_BYTES {
                return Err(Error::Network);
            }
        }
        String::from_utf8(body).map_err(|_| Error::Network)
    }
}

/// Percent-encoding untuk nilai query string (RFC 3986, karakter tak-tercadang dibiarkan).
pub fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_query_values() {
        assert_eq!(encode("Sial (feat. A&B)"), "Sial%20%28feat.%20A%26B%29");
        assert_eq!(encode("Kopi Dangdut"), "Kopi%20Dangdut");
        assert_eq!(encode("é"), "%C3%A9");
    }
}
