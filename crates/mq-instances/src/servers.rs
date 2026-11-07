//! Minecraft `servers.dat` parser, editor, and live Server List Ping client.

use std::fs;
use std::io::{Read, Write};
use std::net::ToSocketAddrs;
use std::path::Path;
use std::time::{Duration, Instant};

use flate2::read::GzDecoder;
use mq_core::{MqError, MqResult};
use serde::{Deserialize, Serialize};

/// Stored multiplayer server entry in `servers.dat`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerEntry {
    /// Human-readable display name.
    pub name: String,
    /// Hostname or IP address with optional port (e.g. `mc.hypixel.net` or `127.0.0.1:25565`).
    pub ip: String,
    /// Optional base64-encoded PNG favicon data URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Whether custom server resource packs are accepted (0 = prompt, 1 = enabled, 2 = disabled).
    #[serde(default = "default_accept_textures")]
    pub accept_textures: u8,
}

fn default_accept_textures() -> u8 {
    1
}

/// Live status result returned from Minecraft Server List Ping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerPingResult {
    /// Round-trip ping latency in milliseconds.
    pub latency_ms: u64,
    /// Server version string (e.g. `"1.21.1"` or `"Paper 1.20.4"`).
    pub version_name: String,
    /// Current number of online players.
    pub players_online: u32,
    /// Maximum player capacity.
    pub players_max: u32,
    /// Message of the day (MOTD) plain text.
    pub motd: String,
    /// Optional base64 favicon data URL.
    pub favicon: Option<String>,
}

/// Loads all multiplayer servers declared in `<instance_dir>/servers.dat`.
pub fn load_servers_dat(instance_dir: &Path) -> MqResult<Vec<ServerEntry>> {
    let dat_path = instance_dir.join("servers.dat");
    if !dat_path.exists() {
        return Ok(Vec::new());
    }

    let raw_bytes = fs::read(&dat_path)?;
    if raw_bytes.is_empty() {
        return Ok(Vec::new());
    }

    // Try gzip decompress, or fallback to raw
    let bytes = if raw_bytes.starts_with(&[0x1F, 0x8B]) {
        let mut gz = GzDecoder::new(&raw_bytes[..]);
        let mut decompressed = Vec::new();
        if gz.read_to_end(&mut decompressed).is_ok() {
            decompressed
        } else {
            raw_bytes
        }
    } else {
        raw_bytes
    };

    parse_servers_nbt(&bytes)
}

/// Saves the list of servers to `<instance_dir>/servers.dat` in standard uncompressed NBT format.
pub fn save_servers_dat(instance_dir: &Path, servers: &[ServerEntry]) -> MqResult<()> {
    let dat_path = instance_dir.join("servers.dat");
    let mut buf = Vec::new();

    // TAG_Compound (0x0A) + empty name (0x00, 0x00)
    buf.push(0x0A);
    buf.extend_from_slice(&[0x00, 0x00]);

    // TAG_List (0x09) + name "servers"
    buf.push(0x09);
    write_nbt_string_header(&mut buf, "servers");
    // List item type: TAG_Compound (0x0A)
    buf.push(0x0A);
    // List size (i32 big-endian)
    buf.extend_from_slice(&(servers.len() as i32).to_be_bytes());

    for s in servers {
        // TAG_String "name"
        buf.push(0x08);
        write_nbt_string_header(&mut buf, "name");
        write_nbt_string_payload(&mut buf, &s.name);

        // TAG_String "ip"
        buf.push(0x08);
        write_nbt_string_header(&mut buf, "ip");
        write_nbt_string_payload(&mut buf, &s.ip);

        // TAG_Byte "acceptTextures"
        buf.push(0x01);
        write_nbt_string_header(&mut buf, "acceptTextures");
        buf.push(s.accept_textures);

        if let Some(ref icon) = s.icon {
            buf.push(0x08);
            write_nbt_string_header(&mut buf, "icon");
            write_nbt_string_payload(&mut buf, icon);
        }

        // TAG_End (0x00)
        buf.push(0x00);
    }

    // TAG_End for root compound
    buf.push(0x00);

    fs::write(&dat_path, buf)?;
    Ok(())
}

fn write_nbt_string_header(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    buf.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
    buf.extend_from_slice(bytes);
}

fn write_nbt_string_payload(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    buf.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
    buf.extend_from_slice(bytes);
}

/// Robust scanner parsing `servers.dat` NBT structures.
fn parse_servers_nbt(bytes: &[u8]) -> MqResult<Vec<ServerEntry>> {
    let mut servers = Vec::new();
    let mut cursor = 0;

    // Fast sequential search for server compounds
    while cursor < bytes.len() {
        let name_pos = match find_subsequence(&bytes[cursor..], b"name") {
            Some(p) => cursor + p,
            None => break,
        };
        let ip_pos = match find_subsequence(&bytes[name_pos..], b"ip") {
            Some(p) => name_pos + p,
            None => break,
        };

        // Extract name
        let name_start = name_pos + 4;
        let mut name = String::new();
        if name_start + 2 <= bytes.len() {
            let len = u16::from_be_bytes([bytes[name_start], bytes[name_start + 1]]) as usize;
            if name_start + 2 + len <= bytes.len() {
                if let Ok(s) = std::str::from_utf8(&bytes[name_start + 2..name_start + 2 + len]) {
                    name = s.to_string();
                }
            }
        }

        // Extract IP
        let ip_start = ip_pos + 2;
        let mut ip = String::new();
        if ip_start + 2 <= bytes.len() {
            let len = u16::from_be_bytes([bytes[ip_start], bytes[ip_start + 1]]) as usize;
            if ip_start + 2 + len <= bytes.len() {
                if let Ok(s) = std::str::from_utf8(&bytes[ip_start + 2..ip_start + 2 + len]) {
                    ip = s.to_string();
                }
            }
        }

        if !ip.is_empty() {
            servers.push(ServerEntry {
                name: if name.is_empty() { ip.clone() } else { name },
                ip,
                icon: None,
                accept_textures: 1,
            });
            cursor = ip_pos + 4;
        } else {
            cursor = name_pos + 4;
        }
    }

    Ok(servers)
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Live Minecraft TCP Server List Ping implementation.
pub fn ping_server(server_addr: &str, timeout_ms: u64) -> MqResult<ServerPingResult> {
    let (host, port) = match server_addr.split_once(':') {
        Some((h, p)) => (h.trim(), p.trim().parse::<u16>().unwrap_or(25565)),
        None => (server_addr.trim(), 25565),
    };

    let start = Instant::now();
    let sock_addrs: Vec<_> = format!("{host}:{port}")
        .to_socket_addrs()
        .map_err(|e| MqError::Network(format!("DNS resolution failed for {host}: {e}")))?
        .collect();

    if sock_addrs.is_empty() {
        return Err(MqError::Network(format!(
            "no IP addresses resolved for {host}"
        )));
    }

    let timeout = Duration::from_millis(timeout_ms);
    let mut stream = std::net::TcpStream::connect_timeout(&sock_addrs[0], timeout)
        .map_err(|e| MqError::Network(format!("connection timed out or refused: {e}")))?;

    stream.set_read_timeout(Some(timeout)).ok();
    stream.set_write_timeout(Some(timeout)).ok();

    // 1. Handshake Packet (ID 0x00)
    let mut handshake_data = Vec::new();
    write_varint(&mut handshake_data, 0x00); // Packet ID
    write_varint(&mut handshake_data, 765); // Protocol version (1.20.4)
    write_string(&mut handshake_data, host);
    handshake_data.extend_from_slice(&port.to_be_bytes());
    write_varint(&mut handshake_data, 1); // Next state: status (1)

    let mut packet = Vec::new();
    write_varint(&mut packet, handshake_data.len() as i32);
    packet.extend_from_slice(&handshake_data);
    stream.write_all(&packet)?;

    // 2. Status Request Packet (ID 0x00)
    let req_packet = [0x01, 0x00];
    stream.write_all(&req_packet)?;
    stream.flush()?;

    // 3. Status Response
    let _packet_len = read_varint(&mut stream)?;
    let packet_id = read_varint(&mut stream)?;
    if packet_id != 0 {
        return Err(MqError::Network(format!(
            "unexpected response packet ID: {packet_id}"
        )));
    }

    let json_len = read_varint(&mut stream)? as usize;
    let mut json_buf = vec![0u8; json_len];
    stream.read_exact(&mut json_buf)?;

    let latency_ms = start.elapsed().as_millis() as u64;
    let json_str = String::from_utf8_lossy(&json_buf);

    let val: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| MqError::Internal(format!("failed parsing server ping response JSON: {e}")))?;

    let version_name = val
        .pointer("/version/name")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown")
        .to_string();

    let players_online = val
        .pointer("/players/online")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    let players_max = val
        .pointer("/players/max")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    let motd = if let Some(desc_str) = val.get("description").and_then(|d| d.as_str()) {
        desc_str.to_string()
    } else if let Some(text) = val.pointer("/description/text").and_then(|t| t.as_str()) {
        text.to_string()
    } else {
        "A Minecraft Server".to_string()
    };

    let favicon = val
        .get("favicon")
        .and_then(|f| f.as_str())
        .map(ToOwned::to_owned);

    Ok(ServerPingResult {
        latency_ms,
        version_name,
        players_online,
        players_max,
        motd,
        favicon,
    })
}

fn write_varint(w: &mut Vec<u8>, mut val: i32) {
    loop {
        if (val & !0x7F) == 0 {
            w.push(val as u8);
            return;
        }
        w.push(((val & 0x7F) | 0x80) as u8);
        val = ((val as u32) >> 7) as i32;
    }
}

fn write_string(w: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    write_varint(w, bytes.len() as i32);
    w.extend_from_slice(bytes);
}

fn read_varint<R: Read>(r: &mut R) -> MqResult<i32> {
    let mut num_read = 0;
    let mut result = 0;
    let mut byte = [0u8; 1];

    loop {
        r.read_exact(&mut byte)?;
        let val = byte[0] & 0x7F;
        result |= (val as i32) << (7 * num_read);
        num_read += 1;
        if num_read > 5 {
            return Err(MqError::Internal("VarInt is too big".to_string()));
        }
        if (byte[0] & 0x80) == 0 {
            break;
        }
    }

    Ok(result)
}
