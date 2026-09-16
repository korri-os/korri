use super::{AndroidActiveLaunch, LaunchContributorKind, LaunchSpec};
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha256;
use std::{
    fmt, io,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket},
    path::Path,
    time::Duration,
};
use zeroize::Zeroizing;

type HmacSha256 = Hmac<Sha256>;

const CONTROL_TIMEOUT: Duration = Duration::from_millis(250);
const KORRI_PACKAGE: &str = "com.korri.retroarch";
const KORRI_ACTIVITY: &str = "com.retroarch.browser.retroactivity.RetroActivityFuture";
const KORRI_RUNNER: &str = "@korri:mgba/mgba";
const KORRI_EXECUTOR: &str = "retroarch-control";
const PROTOCOL_VERSION: u8 = 1;
const NONCE_LENGTH: usize = 32;
const MAC_LENGTH: usize = 32;
const REQUEST_LENGTH: usize = 1 + NONCE_LENGTH + 1 + MAC_LENGTH;
const RESPONSE_PREFIX_LENGTH: usize = 1 + NONCE_LENGTH + 1 + 2;
const MAX_RESULT_LENGTH: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RetroarchControlCommand {
    GetStatus,
    ShowMenu,
    Quit,
}

impl RetroarchControlCommand {
    fn tag(self) -> u8 {
        match self {
            Self::GetStatus => 1,
            Self::ShowMenu => 2,
            Self::Quit => 3,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RetroarchStatus {
    pub(crate) paused: bool,
    pub(crate) system: String,
    pub(crate) content: String,
    pub(crate) crc32: String,
    pub(crate) menu_alive: bool,
    pub(crate) menu_selection: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RetroarchControlResponse {
    Status(RetroarchStatus),
    NotPlaying,
    Acknowledged,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum RetroarchControlError {
    #[error("invalid RetroArch control authority")]
    InvalidAuthority,
    #[error("RetroArch control listener is unavailable")]
    Unavailable,
    #[error("RetroArch control request timed out")]
    Timeout,
    #[error("RetroArch control reply came from the wrong source")]
    WrongSource,
    #[error("RetroArch control reply did not match the request")]
    WrongResponse,
}

pub(crate) struct RetroarchControlAuthority {
    launch_id: String,
    endpoint: SocketAddr,
    token: Zeroizing<[u8; 64]>,
    game_id: String,
    expected_content: String,
    expected_crc32: String,
}

impl RetroarchControlAuthority {
    pub(crate) fn retain_from_verified_launch(
        spec: &LaunchSpec,
        token: &str,
        port: u16,
    ) -> Result<Self, RetroarchControlError> {
        let trusted_shape = spec.runner_id == "retroarch"
            && spec.component.package_name == KORRI_PACKAGE
            && spec.component.class_name == KORRI_ACTIVITY
            && spec.context.contributors.iter().any(|contributor| {
                contributor.kind == LaunchContributorKind::Runner && contributor.id == KORRI_RUNNER
            })
            && spec
                .context
                .executor
                .as_ref()
                .is_some_and(|executor| executor.id == KORRI_EXECUTOR && executor.available);
        let game_id = spec
            .context
            .game_id
            .as_deref()
            .filter(|value| !value.is_empty());
        let expected_content = spec
            .extras
            .get("ROM")
            .and_then(|rom| Path::new(rom).file_name())
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty() && !name.chars().any(char::is_control));
        let expected_crc32 = spec
            .context
            .content_crc32
            .as_deref()
            .filter(|value| valid_crc32(value));
        if !trusted_shape
            || !(49152..=65535).contains(&port)
            || game_id.is_none()
            || expected_content.is_none()
            || expected_crc32.is_none()
        {
            return Err(RetroarchControlError::InvalidAuthority);
        }
        Self::new(
            spec.launch_id.clone(),
            token,
            port,
            game_id.expect("checked").to_owned(),
            expected_content.expect("checked").to_owned(),
            expected_crc32.expect("checked").to_owned(),
        )
    }

    fn new(
        launch_id: impl Into<String>,
        token: &str,
        port: u16,
        game_id: impl Into<String>,
        expected_content: impl Into<String>,
        expected_crc32: impl Into<String>,
    ) -> Result<Self, RetroarchControlError> {
        let expected_crc32 = expected_crc32.into();
        if !valid_token(token) || !(49152..=65535).contains(&port) || !valid_crc32(&expected_crc32)
        {
            return Err(RetroarchControlError::InvalidAuthority);
        }
        let mut retained = [0_u8; 64];
        retained.copy_from_slice(token.as_bytes());
        Ok(Self {
            launch_id: launch_id.into(),
            endpoint: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)),
            token: Zeroizing::new(retained),
            game_id: game_id.into(),
            expected_content: expected_content.into(),
            expected_crc32: expected_crc32.into(),
        })
    }

    pub(crate) fn is_for(&self, launch_id: &str) -> bool {
        self.launch_id == launch_id
    }

    pub(crate) fn port(&self) -> u16 {
        self.endpoint.port()
    }

    #[cfg(target_os = "android")]
    pub(crate) fn token_bytes(&self) -> &[u8] {
        self.token.as_slice()
    }

    pub(crate) fn matches_launch(&self, active: &AndroidActiveLaunch, spec: &LaunchSpec) -> bool {
        self.is_for(&active.launch_id)
            && active.game_id.as_deref() == Some(self.game_id.as_str())
            && active.game_id == spec.context.game_id
            && active.title == spec.context.title
            && active.content_crc32.as_deref() == Some(self.expected_crc32.as_str())
            && active.content_crc32 == spec.context.content_crc32
            && active.contributors == spec.context.contributors
            && active.executor == spec.context.executor
            && active.foreground == spec.context.foreground
            && spec
                .extras
                .get("ROM")
                .and_then(|rom| Path::new(rom).file_name())
                .and_then(|name| name.to_str())
                == Some(self.expected_content.as_str())
    }

    pub(crate) fn expected_status(&self) -> Result<Option<RetroarchStatus>, RetroarchControlError> {
        match self.request(RetroarchControlCommand::GetStatus)? {
            RetroarchControlResponse::Status(status)
                if status.content == self.expected_content
                    && status.crc32 == self.expected_crc32 =>
            {
                Ok(Some(status))
            }
            RetroarchControlResponse::Status(_)
            | RetroarchControlResponse::NotPlaying
            | RetroarchControlResponse::Acknowledged => Ok(None),
        }
    }

    pub(crate) fn confirms_expected_content(&self) -> Result<bool, RetroarchControlError> {
        self.expected_status().map(|status| status.is_some())
    }

    pub(crate) fn invoke(
        &self,
        command: RetroarchControlCommand,
    ) -> Result<(), RetroarchControlError> {
        if command == RetroarchControlCommand::GetStatus {
            return Err(RetroarchControlError::WrongResponse);
        }
        match self.request(command)? {
            RetroarchControlResponse::Acknowledged => Ok(()),
            RetroarchControlResponse::Status(_) | RetroarchControlResponse::NotPlaying => {
                Err(RetroarchControlError::WrongResponse)
            }
        }
    }

    fn request(
        &self,
        command: RetroarchControlCommand,
    ) -> Result<RetroarchControlResponse, RetroarchControlError> {
        let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
            .map_err(|_| RetroarchControlError::Unavailable)?;
        socket
            .set_read_timeout(Some(CONTROL_TIMEOUT))
            .map_err(|_| RetroarchControlError::Unavailable)?;
        request_with_socket(&socket, self.endpoint, &self.token, command)
    }
}

impl fmt::Debug for RetroarchControlAuthority {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RetroarchControlAuthority")
            .field("launch_id", &self.launch_id)
            .field("endpoint", &self.endpoint)
            .field("token", &"[REDACTED]")
            .field("expected_content", &self.expected_content)
            .field("expected_crc32", &self.expected_crc32)
            .finish()
    }
}

fn valid_token(token: &str) -> bool {
    token.len() == 64
        && token
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_crc32(value: &str) -> bool {
    value.len() == 8
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn request_payload(
    token: &[u8; 64],
    nonce: [u8; NONCE_LENGTH],
    command: RetroarchControlCommand,
) -> [u8; REQUEST_LENGTH] {
    let mut payload = [0_u8; REQUEST_LENGTH];
    payload[0] = PROTOCOL_VERSION;
    payload[1..1 + NONCE_LENGTH].copy_from_slice(&nonce);
    payload[1 + NONCE_LENGTH] = command.tag();
    let mut mac = HmacSha256::new_from_slice(token).expect("HMAC accepts a 64-byte key");
    mac.update(&payload[..1 + NONCE_LENGTH + 1]);
    payload[1 + NONCE_LENGTH + 1..].copy_from_slice(&mac.finalize().into_bytes());
    payload
}

fn request_with_socket(
    socket: &UdpSocket,
    endpoint: SocketAddr,
    token: &[u8; 64],
    command: RetroarchControlCommand,
) -> Result<RetroarchControlResponse, RetroarchControlError> {
    let mut nonce = [0_u8; NONCE_LENGTH];
    rand::rng().fill_bytes(&mut nonce);
    let payload = request_payload(token, nonce, command);
    socket
        .send_to(&payload, endpoint)
        .map_err(|_| RetroarchControlError::Unavailable)?;
    let mut reply = [0_u8; RESPONSE_PREFIX_LENGTH + MAX_RESULT_LENGTH + MAC_LENGTH];
    let (length, source) = socket
        .recv_from(&mut reply)
        .map_err(|error| match error.kind() {
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => RetroarchControlError::Timeout,
            _ => RetroarchControlError::Unavailable,
        })?;
    if source != endpoint {
        return Err(RetroarchControlError::WrongSource);
    }
    parse_authenticated_response(token, nonce, command, &reply[..length])
}

#[cfg(test)]
pub(crate) fn authenticated_response(
    token: &[u8; 64],
    nonce: [u8; NONCE_LENGTH],
    command: RetroarchControlCommand,
    result: &[u8],
) -> Vec<u8> {
    assert!(result.len() <= MAX_RESULT_LENGTH);
    let mut response = Vec::with_capacity(RESPONSE_PREFIX_LENGTH + result.len() + MAC_LENGTH);
    response.push(PROTOCOL_VERSION);
    response.extend_from_slice(&nonce);
    response.push(command.tag());
    response.extend_from_slice(&(result.len() as u16).to_be_bytes());
    response.extend_from_slice(result);
    let mut mac = HmacSha256::new_from_slice(token).expect("HMAC accepts a 64-byte key");
    mac.update(&response);
    response.extend_from_slice(&mac.finalize().into_bytes());
    response
}

fn parse_authenticated_response(
    token: &[u8; 64],
    nonce: [u8; NONCE_LENGTH],
    command: RetroarchControlCommand,
    response: &[u8],
) -> Result<RetroarchControlResponse, RetroarchControlError> {
    if response.len() < RESPONSE_PREFIX_LENGTH + MAC_LENGTH
        || response[0] != PROTOCOL_VERSION
        || response[1..1 + NONCE_LENGTH] != nonce
        || response[1 + NONCE_LENGTH] != command.tag()
    {
        return Err(RetroarchControlError::WrongResponse);
    }
    let result_length = u16::from_be_bytes([
        response[1 + NONCE_LENGTH + 1],
        response[1 + NONCE_LENGTH + 2],
    ]) as usize;
    let framed_length = RESPONSE_PREFIX_LENGTH + result_length;
    if result_length > MAX_RESULT_LENGTH || response.len() != framed_length + MAC_LENGTH {
        return Err(RetroarchControlError::WrongResponse);
    }
    let mut mac = HmacSha256::new_from_slice(token).expect("HMAC accepts a 64-byte key");
    mac.update(&response[..framed_length]);
    mac.verify_slice(&response[framed_length..])
        .map_err(|_| RetroarchControlError::WrongResponse)?;
    parse_result(command, &response[RESPONSE_PREFIX_LENGTH..framed_length])
}

fn parse_menu_telemetry(value: &str) -> Result<(bool, usize), RetroarchControlError> {
    let (menu, selection) = value
        .split_once(",selection=")
        .ok_or(RetroarchControlError::WrongResponse)?;
    let menu_alive = match menu {
        "0" => false,
        "1" => true,
        _ => return Err(RetroarchControlError::WrongResponse),
    };
    if selection.is_empty()
        || (selection.len() > 1 && selection.starts_with('0'))
        || !selection.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(RetroarchControlError::WrongResponse);
    }
    let selection = selection
        .parse::<usize>()
        .map_err(|_| RetroarchControlError::WrongResponse)?;
    Ok((menu_alive, selection))
}

fn parse_result(
    command: RetroarchControlCommand,
    response: &[u8],
) -> Result<RetroarchControlResponse, RetroarchControlError> {
    match command {
        RetroarchControlCommand::ShowMenu if response == b"SHOW_MENU OK" => {
            Ok(RetroarchControlResponse::Acknowledged)
        }
        RetroarchControlCommand::Quit if response == b"QUIT OK" => {
            Ok(RetroarchControlResponse::Acknowledged)
        }
        RetroarchControlCommand::GetStatus => {
            let response = response
                .strip_suffix(b"\n")
                .ok_or(RetroarchControlError::WrongResponse)?;
            let text =
                std::str::from_utf8(response).map_err(|_| RetroarchControlError::WrongResponse)?;
            if let Some(telemetry) = text.strip_prefix("GET_STATUS CONTENTLESS ") {
                parse_menu_telemetry(telemetry)?;
                return Ok(RetroarchControlResponse::NotPlaying);
            }
            let (paused, details) = if let Some(details) = text.strip_prefix("GET_STATUS PLAYING ")
            {
                (false, details)
            } else if let Some(details) = text.strip_prefix("GET_STATUS PAUSED ") {
                (true, details)
            } else {
                return Err(RetroarchControlError::WrongResponse);
            };
            let (identity, telemetry) = details
                .rsplit_once(",menu=")
                .ok_or(RetroarchControlError::WrongResponse)?;
            let (menu_alive, menu_selection) = parse_menu_telemetry(telemetry)?;
            let (identity, crc32) = identity
                .rsplit_once(",crc32=")
                .ok_or(RetroarchControlError::WrongResponse)?;
            let (system, content) = identity
                .split_once(',')
                .ok_or(RetroarchControlError::WrongResponse)?;
            if [system, content, crc32]
                .iter()
                .any(|value| value.is_empty() || value.chars().any(char::is_control))
                || !valid_crc32(crc32)
                || Path::new(content)
                    .file_name()
                    .and_then(|name| name.to_str())
                    != Some(content)
            {
                return Err(RetroarchControlError::WrongResponse);
            }
            Ok(RetroarchControlResponse::Status(RetroarchStatus {
                paused,
                system: system.to_owned(),
                content: content.to_owned(),
                crc32: crc32.to_owned(),
                menu_alive,
                menu_selection,
            }))
        }
        RetroarchControlCommand::ShowMenu | RetroarchControlCommand::Quit => {
            Err(RetroarchControlError::WrongResponse)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{net::UdpSocket, thread, time::Duration};

    const TOKEN_TEXT: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const TOKEN: &[u8; 64] = b"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn request_is_fixed_size_nonce_bound_and_authenticated() {
        let nonce = [7_u8; NONCE_LENGTH];
        let request = request_payload(TOKEN, nonce, RetroarchControlCommand::ShowMenu);
        assert_eq!(request.len(), REQUEST_LENGTH);
        assert_eq!(request[0], PROTOCOL_VERSION);
        assert_eq!(&request[1..33], &nonce);
        assert_eq!(request[33], RetroarchControlCommand::ShowMenu.tag());
        // Known answer for HMAC-SHA256 over the exact 34-byte binary prefix,
        // keyed by the 64 ASCII token bytes. RetroArch's pinned C parity uses
        // this same frame/key treaty through sha256_hash-based HMAC.
        assert_eq!(
            &request[34..],
            &[
                0xd0, 0xf5, 0x44, 0xda, 0x97, 0x4b, 0x12, 0x49, 0x39, 0x16, 0x19, 0x0c, 0x0b, 0xb6,
                0xa9, 0xb1, 0x16, 0x9c, 0x4e, 0xb4, 0x6b, 0xea, 0x37, 0x9b, 0x9d, 0xa3, 0xba, 0xb4,
                0x93, 0xd3, 0xb5, 0x07,
            ]
        );

        let mut tampered = request;
        tampered[33] = RetroarchControlCommand::Quit.tag();
        let mut mac = HmacSha256::new_from_slice(TOKEN).unwrap();
        mac.update(&tampered[..34]);
        assert!(mac.verify_slice(&tampered[34..]).is_err());
        assert!(!request.windows(TOKEN.len()).any(|window| window == TOKEN));
    }

    #[test]
    fn responses_require_exact_nonce_command_length_and_mac() {
        let nonce = [9_u8; NONCE_LENGTH];
        let valid = authenticated_response(
            TOKEN,
            nonce,
            RetroarchControlCommand::ShowMenu,
            b"SHOW_MENU OK",
        );
        assert_eq!(
            parse_authenticated_response(TOKEN, nonce, RetroarchControlCommand::ShowMenu, &valid),
            Ok(RetroarchControlResponse::Acknowledged)
        );
        for mutation in 0..4 {
            let mut invalid = valid.clone();
            match mutation {
                0 => invalid[1] ^= 1,
                1 => invalid[33] = RetroarchControlCommand::Quit.tag(),
                2 => invalid[35] = invalid[35].wrapping_add(1),
                _ => *invalid.last_mut().unwrap() ^= 1,
            }
            assert_eq!(
                parse_authenticated_response(
                    TOKEN,
                    nonce,
                    RetroarchControlCommand::ShowMenu,
                    &invalid
                ),
                Err(RetroarchControlError::WrongResponse)
            );
        }
        let mut trailing = valid;
        trailing.push(0);
        assert_eq!(
            parse_authenticated_response(
                TOKEN,
                nonce,
                RetroarchControlCommand::ShowMenu,
                &trailing
            ),
            Err(RetroarchControlError::WrongResponse)
        );
    }

    #[test]
    fn status_parser_preserves_content_identity() {
        assert_eq!(
            parse_result(
                RetroarchControlCommand::GetStatus,
                b"GET_STATUS PLAYING mGBA,wl4.gba,crc32=d6141609,menu=1,selection=3\n"
            ),
            Ok(RetroarchControlResponse::Status(RetroarchStatus {
                paused: false,
                system: "mGBA".into(),
                content: "wl4.gba".into(),
                crc32: "d6141609".into(),
                menu_alive: true,
                menu_selection: 3,
            }))
        );
        assert!(parse_result(
            RetroarchControlCommand::GetStatus,
            b"GET_STATUS PLAYING mGBA,other.gba,crc32=d6141609,menu=0,selection=0\n"
        )
        .is_ok());
        for invalid in [
            b"GET_STATUS PLAYING mGBA,../wl4.gba,crc32=d6141609,menu=0,selection=0\n".as_slice(),
            b"GET_STATUS PLAYING mGBA,wl4.gba,crc32=nope,menu=0,selection=0\n".as_slice(),
            b"GET_STATUS PLAYING mGBA,wl4.gba,crc32=1234567,menu=0,selection=0\n".as_slice(),
            b"GET_STATUS PLAYING mGBA,wl4.gba,crc32=D6141609,menu=0,selection=0\n".as_slice(),
            b"GET_STATUS PLAYING mGBA,wl4.gba,crc32=d6141609,menu=2,selection=0\n".as_slice(),
            b"GET_STATUS PLAYING mGBA,wl4.gba,crc32=d6141609,menu=1,selection=03\n".as_slice(),
            b"SHOW_MENU OK".as_slice(),
        ] {
            assert_eq!(
                parse_result(RetroarchControlCommand::GetStatus, invalid),
                Err(RetroarchControlError::WrongResponse)
            );
        }
    }

    fn launch_spec() -> LaunchSpec {
        LaunchSpec {
            launch_id: "launch-1".into(),
            runner_id: "retroarch".into(),
            disposition: super::super::LaunchDisposition::Fresh,
            context: super::super::LaunchContext {
                game_id: Some("wl4".into()),
                title: Some("Wario Land 4".into()),
                content_crc32: Some("d6141609".into()),
                contributors: vec![super::super::LaunchRouteContributor {
                    kind: LaunchContributorKind::Runner,
                    id: KORRI_RUNNER.into(),
                }],
                executor: Some(super::super::LaunchExecutor {
                    id: KORRI_EXECUTOR.into(),
                    available: true,
                }),
                foreground: super::super::LaunchForegroundRule {
                    kind: super::super::LaunchForegroundKind::Component,
                    package_name: Some(KORRI_PACKAGE.into()),
                    class_name: Some(KORRI_ACTIVITY.into()),
                },
            },
            component: super::super::AndroidComponent {
                package_name: KORRI_PACKAGE.into(),
                class_name: KORRI_ACTIVITY.into(),
            },
            extras: std::collections::HashMap::from([(
                "ROM".into(),
                "/storage/emulated/0/korri/roms/wl4.gba".into(),
            )]),
            authorized_content_root: None,
            directories: vec![],
            files: vec![],
            integrity: "verified outside this module".into(),
        }
    }

    #[test]
    fn authority_accepts_only_exact_launch_endpoint_and_expected_content() {
        let authority = RetroarchControlAuthority::retain_from_verified_launch(
            &launch_spec(),
            TOKEN_TEXT,
            50000,
        )
        .unwrap();
        assert_eq!(authority.port(), 50000);
        let spec = launch_spec();
        let active =
            AndroidActiveLaunch::from_context(spec.launch_id.clone(), spec.context.clone());
        assert!(authority.matches_launch(&active, &spec));
        let mut different = spec;
        different
            .extras
            .insert("ROM".into(), "/roms/other.gba".into());
        assert!(!authority.matches_launch(&active, &different));
        assert!(RetroarchControlAuthority::retain_from_verified_launch(
            &launch_spec(),
            TOKEN_TEXT,
            40000
        )
        .is_err());
    }

    #[test]
    fn authority_debug_output_redacts_the_token() {
        let authority = RetroarchControlAuthority::new(
            "launch-1", TOKEN_TEXT, 50000, "wl4", "wl4.gba", "d6141609",
        )
        .unwrap();
        let debug = format!("{authority:?}");
        assert!(debug.contains("launch-1"));
        assert!(!debug.contains(TOKEN_TEXT));
        assert!(debug.contains("[REDACTED]"));
    }

    #[test]
    fn udp_request_rejects_wrong_source_and_wrong_authenticated_reply() {
        let expected = UdpSocket::bind("127.0.0.1:0").unwrap();
        let endpoint = expected.local_addr().unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        let client = UdpSocket::bind("127.0.0.1:0").unwrap();
        client
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let client_address = client.local_addr().unwrap();
        thread::spawn(move || {
            let mut request = [0_u8; REQUEST_LENGTH];
            expected.recv_from(&mut request).unwrap();
            let mut nonce = [0_u8; NONCE_LENGTH];
            nonce.copy_from_slice(&request[1..33]);
            let reply = authenticated_response(
                TOKEN,
                nonce,
                RetroarchControlCommand::ShowMenu,
                b"SHOW_MENU OK",
            );
            sender.send_to(&reply, client_address).unwrap();
        });
        assert_eq!(
            request_with_socket(&client, endpoint, TOKEN, RetroarchControlCommand::ShowMenu),
            Err(RetroarchControlError::WrongSource)
        );

        let server = UdpSocket::bind("127.0.0.1:0").unwrap();
        let endpoint = server.local_addr().unwrap();
        thread::spawn(move || {
            let mut request = [0_u8; REQUEST_LENGTH];
            let (_, source) = server.recv_from(&mut request).unwrap();
            let mut nonce = [0_u8; NONCE_LENGTH];
            nonce.copy_from_slice(&request[1..33]);
            let reply =
                authenticated_response(TOKEN, nonce, RetroarchControlCommand::Quit, b"QUIT OK");
            server.send_to(&reply, source).unwrap();
        });
        let client = UdpSocket::bind("127.0.0.1:0").unwrap();
        client
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        assert_eq!(
            request_with_socket(&client, endpoint, TOKEN, RetroarchControlCommand::ShowMenu),
            Err(RetroarchControlError::WrongResponse)
        );
    }

    #[test]
    fn authority_rejects_live_status_for_different_content() {
        let server = (49152..=65535)
            .find_map(|port| UdpSocket::bind(("127.0.0.1", port)).ok())
            .unwrap();
        let port = server.local_addr().unwrap().port();
        let authority = RetroarchControlAuthority::new(
            "launch-1", TOKEN_TEXT, port, "wl4", "wl4.gba", "d6141609",
        )
        .unwrap();
        let responder = thread::spawn(move || {
            let mut request = [0_u8; REQUEST_LENGTH];
            let (_, source) = server.recv_from(&mut request).unwrap();
            let mut nonce = [0_u8; NONCE_LENGTH];
            nonce.copy_from_slice(&request[1..33]);
            let reply = authenticated_response(
                TOKEN,
                nonce,
                RetroarchControlCommand::GetStatus,
                b"GET_STATUS PLAYING mGBA,stale.gba,crc32=d6141609,menu=0,selection=0\n",
            );
            server.send_to(&reply, source).unwrap();
        });

        assert_eq!(authority.confirms_expected_content(), Ok(false));
        responder.join().unwrap();
    }

    #[test]
    fn authority_rejects_live_status_for_replaced_same_basename() {
        let server = (49152..=65535)
            .find_map(|port| UdpSocket::bind(("127.0.0.1", port)).ok())
            .unwrap();
        let port = server.local_addr().unwrap().port();
        let authority = RetroarchControlAuthority::new(
            "launch-1", TOKEN_TEXT, port, "wl4", "wl4.gba", "d6141609",
        )
        .unwrap();
        let responder = thread::spawn(move || {
            let mut request = [0_u8; REQUEST_LENGTH];
            let (_, source) = server.recv_from(&mut request).unwrap();
            let mut nonce = [0_u8; NONCE_LENGTH];
            nonce.copy_from_slice(&request[1..33]);
            let reply = authenticated_response(
                TOKEN,
                nonce,
                RetroarchControlCommand::GetStatus,
                b"GET_STATUS PLAYING mGBA,wl4.gba,crc32=deadbeef,menu=0,selection=0\n",
            );
            server.send_to(&reply, source).unwrap();
        });

        assert_eq!(authority.confirms_expected_content(), Ok(false));
        responder.join().unwrap();
    }

    #[test]
    fn udp_request_times_out_within_the_configured_bound() {
        let silent = UdpSocket::bind("127.0.0.1:0").unwrap();
        let client = UdpSocket::bind("127.0.0.1:0").unwrap();
        client
            .set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        assert_eq!(
            request_with_socket(
                &client,
                silent.local_addr().unwrap(),
                TOKEN,
                RetroarchControlCommand::GetStatus,
            ),
            Err(RetroarchControlError::Timeout)
        );
    }
}
