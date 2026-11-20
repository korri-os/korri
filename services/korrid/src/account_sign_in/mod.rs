//! Account sign-in (CONTEXT.md, ADR 0089): one plugin-neutral treaty value,
//! `SurfaceAccountSignIn`, for every plugin's link to an outside service.
//! Steam fills it from its own engine host (ADR 0083, `steam_engine`); every
//! other plugin that declares an `account` fills it through the generic account
//! host here. korrid makes each QR code itself (`qr`).
pub mod job;
pub mod protocol;
pub mod qr;
pub mod state;
