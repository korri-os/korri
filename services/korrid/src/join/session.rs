//! The two sides of one join. Each returns the join's last status; the
//! coordinator stores it. Any failure ends the join or, on the owned device,
//! the code.

use super::{
    code::{choose_nameplate, JoinCode, Nameplate, ShownCode},
    cpace::{Initiator, Responder},
    failure,
    rendezvous::{Message, Open, Rendezvous, Step},
    JoinCodeShown, JoinContext, JoinStatus, JoinedDevice, JoiningDevice, Reporter,
};
use crate::identity::{DeviceIdentity, IdentityState, OwnerStatementStatus};

const UNREACHABLE: &str = "Korri could not reach the relays. Check the network and try again.";
const MISMATCH: &str = "The code did not match. Check the code and try again.";
const NO_ANSWER: &str = "The other device stopped answering. Try again with a new code.";

fn failed(code: &str, message: &str) -> JoinStatus {
    JoinStatus::Failed(failure(code, message))
}

/// How one shown code ended.
enum Round {
    /// It expired, or its one guess came too late: show a new code.
    Expired,
    /// A wrong code was tried: show a new code and say so.
    Mismatch,
    /// No one watches the join any more.
    Unwatched,
    Done(JoinStatus),
}

/// The owned device: show codes until one matches, ask for Accept, vouch.
pub(super) async fn owned(
    context: &JoinContext,
    rendezvous: &Rendezvous,
    reporter: &Reporter,
    mut decision: tokio::sync::oneshot::Receiver<bool>,
) -> JoinStatus {
    let mut wrong_code_tried = false;
    loop {
        let now = (context.clock)();
        if reporter.unwatched(now) {
            return JoinStatus::Idle;
        }
        let lifetime = context.timing.code_lifetime;
        let Ok(in_use) = rendezvous.nameplates_in_use(now, lifetime).await else {
            return failed("JoinRelaysUnavailable", UNREACHABLE);
        };
        let Some(nameplate) = choose_nameplate(&in_use, &mut rand::rng()) else {
            return failed(
                "JoinNameplatesBusy",
                "Every join code is in use right now. Try again in a few minutes.",
            );
        };
        let mut shown = ShownCode::new(
            JoinCode::generate(nameplate, &mut rand::rng()),
            now + lifetime,
        );
        let Ok(open) = rendezvous.open(nameplate, now, shown.expires_at()).await else {
            return failed("JoinRelaysUnavailable", UNREACHABLE);
        };
        reporter.set(JoinStatus::ShowingCode(JoinCodeShown {
            code: shown.code().shown(),
            expires_at: shown.expires_at(),
            wrong_code_tried,
        }));
        match code_round(
            context,
            rendezvous,
            reporter,
            &mut shown,
            &open,
            &mut decision,
        )
        .await
        {
            Round::Expired => wrong_code_tried = false,
            Round::Mismatch => wrong_code_tried = true,
            Round::Unwatched => return JoinStatus::Idle,
            Round::Done(status) => return status,
        }
    }
}

async fn code_round(
    context: &JoinContext,
    rendezvous: &Rendezvous,
    reporter: &Reporter,
    shown: &mut ShownCode,
    open: &Open,
    decision: &mut tokio::sync::oneshot::Receiver<bool>,
) -> Round {
    let nameplate = shown.code().nameplate();
    let own = open.owned_device.clone();
    let expires_at = shown.expires_at();
    let clock = || (context.clock)();
    // The first device that names this open event takes the code's guess.
    let (peer, hello) = loop {
        let now = clock();
        if shown.is_expired(now) {
            return Round::Expired;
        }
        if reporter.unwatched(now) {
            return Round::Unwatched;
        }
        if let Ok(messages) = rendezvous
            .receive(nameplate, None, &[Step::Hello], open.created_at, now)
            .await
        {
            if let Some(received) = messages.into_iter().find(|received| {
                matches!(&received.message, Message::Hello { open: id } if *id == open.id)
                    && received.author != own
            }) {
                break (received.author, received.id);
            }
        }
        tokio::time::sleep(context.timing.poll).await;
    };
    let Ok((initiator, offer)) = Initiator::start(&shown.code().secret(), &own, &peer) else {
        return Round::Done(failed(
            "JoinFailed",
            "Korri could not start the key exchange.",
        ));
    };
    let session = hex::encode(initiator.session());
    let offer = Message::Offer {
        hello,
        message: hex::encode(offer),
    };
    if rendezvous
        .send(nameplate, &peer, &offer, clock(), expires_at)
        .await
        .is_err()
    {
        return Round::Done(failed("JoinRelaysUnavailable", UNREACHABLE));
    }
    // A device that says hello and never answers would hold the code until
    // it expires. After a short wait its hello counts as the code's guess.
    let answer_by = clock().saturating_add(context.timing.answer_wait);
    let (message, confirmation, device) = loop {
        let now = clock();
        if shown.is_expired(now) {
            return Round::Expired;
        }
        if reporter.unwatched(now) {
            return Round::Unwatched;
        }
        if now >= answer_by {
            let _ = shown.take_guess(now);
            let mismatch = Message::Mismatch { session };
            let _ = rendezvous
                .send(nameplate, &peer, &mismatch, now, expires_at)
                .await;
            return Round::Mismatch;
        }
        if let Ok(messages) = rendezvous
            .receive(
                nameplate,
                Some(&peer),
                &[Step::Answer],
                open.created_at,
                now,
            )
            .await
        {
            let answer = messages
                .into_iter()
                .find_map(|received| match received.message {
                    Message::Answer {
                        session: answered,
                        message,
                        confirmation,
                        device,
                    } if answered == session => Some((message, confirmation, device)),
                    _ => None,
                });
            if let Some(answer) = answer {
                break answer;
            }
        }
        tokio::time::sleep(context.timing.poll).await;
    };
    // The code's one guess, whatever its result.
    if shown.take_guess(clock()).is_err() {
        return Round::Expired;
    }
    let confirmed = match (hex::decode(message), hex::decode(confirmation)) {
        (Ok(message), Ok(confirmation)) => initiator.finish(&message, &confirmation).ok(),
        _ => None,
    };
    let Some(own_confirmation) = confirmed else {
        let mismatch = Message::Mismatch { session };
        let _ = rendezvous
            .send(nameplate, &peer, &mismatch, clock(), expires_at)
            .await;
        return Round::Mismatch;
    };
    let Some(device) = device.checked() else {
        return Round::Done(failed(
            "JoinDeviceFactsInvalid",
            "The joining device sent facts that Korri cannot show.",
        ));
    };
    let decided_by = clock() + context.timing.accept_wait;
    let confirm = Message::Confirm {
        session: session.clone(),
        confirmation: hex::encode(own_confirmation),
    };
    if rendezvous
        .send(nameplate, &peer, &confirm, clock(), decided_by)
        .await
        .is_err()
    {
        return Round::Done(failed("JoinRelaysUnavailable", UNREACHABLE));
    }
    reporter.set(JoinStatus::AwaitingAccept(device));
    let accepted = loop {
        tokio::select! {
            answer = &mut *decision => break answer.unwrap_or(false),
            () = tokio::time::sleep(context.timing.poll) => {
                let now = clock();
                if now >= decided_by || reporter.unwatched(now) {
                    break false;
                }
            }
        }
    };
    if !accepted {
        let _ = rendezvous
            .send(
                nameplate,
                &peer,
                &Message::Refuse { session },
                clock(),
                decided_by,
            )
            .await;
        return Round::Done(JoinStatus::Refused);
    }
    Round::Done(vouch(context, rendezvous, nameplate, &peer, session, decided_by).await)
}

async fn vouch(
    context: &JoinContext,
    rendezvous: &Rendezvous,
    nameplate: Nameplate,
    peer: &str,
    session: String,
    expires_at: u64,
) -> JoinStatus {
    // The identity as it is now, not as it was when the code was shown: it
    // may have a newer owner statement, or no owner at all.
    let Ok(identity) = context.credentials.identity_snapshot() else {
        return failed("IdentityUnavailable", "Device identity is unavailable.");
    };
    let IdentityState::Owned {
        owner_public_key, ..
    } = identity.state()
    else {
        return failed("JoinNeedsOwner", "This device no longer has an owner.");
    };
    let Ok(vouch) = identity.vouch_for(peer, (context.clock)()) else {
        return failed("JoinFailed", "Korri could not sign the vouch.");
    };
    // ADR 0069: vouches are public on the relays, so every peer can follow
    // the new device's chain.
    if rendezvous.publish(&vouch.event_json).await.is_err() {
        return failed("JoinRelaysUnavailable", UNREACHABLE);
    }
    let message = Message::Vouch {
        session,
        owner_statement: vouch.owner_statement.clone(),
    };
    if rendezvous
        .send(nameplate, peer, &message, (context.clock)(), expires_at)
        .await
        .is_err()
    {
        return failed("JoinRelaysUnavailable", UNREACHABLE);
    }
    // The device is a peer now. Discovery would read the vouch from the
    // relays on its next pass; this saves the wait.
    if let Some(directory) = &context.directory {
        if let Ok(token) = directory.begin_work() {
            let _ = directory.apply_membership(&token, &[vouch.owner_statement]);
        }
    }
    JoinStatus::Joined(JoinedDevice {
        device_public_key: peer.into(),
        owner_public_key: owner_public_key.clone(),
    })
}

/// The new device: find the owned device, check the code, wait for its
/// vouch and store it.
pub(super) async fn joining(
    context: &JoinContext,
    rendezvous: &Rendezvous,
    reporter: &Reporter,
    code: JoinCode,
    device: JoiningDevice,
) -> JoinStatus {
    let clock = || (context.clock)();
    let Some(own) = rendezvous.identity().device_public_key().map(str::to_owned) else {
        return failed("IdentityUnavailable", "Device identity is unavailable.");
    };
    let nameplate = code.nameplate();
    let lifetime = context.timing.code_lifetime;
    let started = clock();
    let deadline = started + lifetime;
    let open = loop {
        let now = clock();
        if now >= deadline {
            return failed(
                "JoinCodeNotFound",
                "No device shows this code. Check the code and try again.",
            );
        }
        if let Ok(Some(open)) = rendezvous.find_open(nameplate, now, lifetime).await {
            break open;
        }
        tokio::time::sleep(context.timing.poll).await;
    };
    let peer = open.owned_device.clone();
    let hello = Message::Hello {
        open: open.id.clone(),
    };
    let Ok(hello) = rendezvous
        .send(nameplate, &peer, &hello, clock(), deadline)
        .await
    else {
        return failed("JoinRelaysUnavailable", UNREACHABLE);
    };
    let offer = match wait_for(
        context,
        rendezvous,
        nameplate,
        &peer,
        &[Step::Offer],
        started,
        deadline,
        |message| matches!(message, Message::Offer { hello: id, .. } if *id == hello),
    )
    .await
    {
        Some(Message::Offer { message, .. }) => message,
        _ => return failed("JoinNoAnswer", NO_ANSWER),
    };
    let Ok(offer) = hex::decode(offer) else {
        return failed("JoinCodeMismatch", MISMATCH);
    };
    let Ok((responder, answer)) = Responder::answer(&code.secret(), &peer, &own, &offer) else {
        return failed("JoinCodeMismatch", MISMATCH);
    };
    let session = hex::encode(responder.session());
    let answer = Message::Answer {
        session: session.clone(),
        message: hex::encode(answer.message),
        confirmation: hex::encode(answer.confirmation),
        device,
    };
    if rendezvous
        .send(nameplate, &peer, &answer, clock(), deadline)
        .await
        .is_err()
    {
        return failed("JoinRelaysUnavailable", UNREACHABLE);
    }
    let same_session = |message: &Message| match message {
        Message::Confirm { session: other, .. }
        | Message::Mismatch { session: other }
        | Message::Vouch { session: other, .. }
        | Message::Refuse { session: other } => *other == session,
        _ => false,
    };
    let confirmation = match wait_for(
        context,
        rendezvous,
        nameplate,
        &peer,
        &[Step::Confirm, Step::Mismatch],
        started,
        deadline,
        same_session,
    )
    .await
    {
        Some(Message::Confirm { confirmation, .. }) => confirmation,
        Some(_) => return failed("JoinCodeMismatch", MISMATCH),
        None => return failed("JoinNoAnswer", NO_ANSWER),
    };
    let checked = hex::decode(confirmation)
        .ok()
        .and_then(|confirmation| responder.confirm(&confirmation).ok());
    if checked.is_none() {
        return failed("JoinCodeMismatch", MISMATCH);
    }
    reporter.set(JoinStatus::WaitingForAccept);
    let decided_by = clock() + context.timing.accept_wait;
    let owner_statement = match wait_for(
        context,
        rendezvous,
        nameplate,
        &peer,
        &[Step::Vouch, Step::Refuse],
        started,
        decided_by,
        same_session,
    )
    .await
    {
        Some(Message::Vouch {
            owner_statement, ..
        }) => owner_statement,
        Some(_) => return JoinStatus::Refused,
        None => return failed("JoinNoAnswer", NO_ANSWER),
    };
    store_vouch(context, &own, &peer, &owner_statement).await
}

/// Store the vouch chain the owned device sent, after checking that this
/// exact device signed its first link. It holds the owner-write lock that
/// setup's end also holds, and reads the identity again under it: a device
/// that got its automatic owner in the meantime refuses the vouch.
pub(super) async fn store_vouch(
    context: &JoinContext,
    own: &str,
    peer: &str,
    owner_statement: &str,
) -> JoinStatus {
    let invalid = || {
        failed(
            "JoinVouchInvalid",
            "The other device sent a vouch that Korri cannot check.",
        )
    };
    let Ok(statement) = DeviceIdentity::verify_owner_statement(owner_statement, own) else {
        return invalid();
    };
    if statement.status != OwnerStatementStatus::Owned
        || statement.vouchers.first().map(String::as_str) != Some(peer)
    {
        return invalid();
    }
    let _owner_write = crate::identity::owner_write().await;
    let stored =
        DeviceIdentity::load_or_create(&context.private_state_root).and_then(|mut identity| {
            match identity.state() {
                IdentityState::Unowned { .. } => identity.apply_owner_statement(owner_statement),
                _ => Err(crate::identity::IdentityError::Invalid(
                    "the device got an owner during the join".into(),
                )),
            }
        });
    if stored.is_err() {
        return failed(
            "JoinStoreFailed",
            "This device could not store its new owner. Try again with a new code.",
        );
    }
    let _ = context
        .credentials
        .reload_identity(&context.private_state_root);
    JoinStatus::Joined(JoinedDevice {
        device_public_key: own.into(),
        owner_public_key: statement.owner_public_key,
    })
}

/// The first message from the peer that fits, or None at the deadline.
#[allow(clippy::too_many_arguments)]
async fn wait_for(
    context: &JoinContext,
    rendezvous: &Rendezvous,
    nameplate: Nameplate,
    peer: &str,
    steps: &[Step],
    since: u64,
    deadline: u64,
    fits: impl Fn(&Message) -> bool,
) -> Option<Message> {
    loop {
        let now = (context.clock)();
        if now >= deadline {
            return None;
        }
        if let Ok(messages) = rendezvous
            .receive(nameplate, Some(peer), steps, since, now)
            .await
        {
            if let Some(received) = messages
                .into_iter()
                .find(|received| fits(&received.message))
            {
                return Some(received.message);
            }
        }
        tokio::time::sleep(context.timing.poll).await;
    }
}
