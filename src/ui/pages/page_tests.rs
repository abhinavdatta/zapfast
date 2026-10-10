//! Smoke and regression tests for the roster pages and their overlays.
//!
//! These drive the complete, real UI frame (`App::frame_ui`, which runs
//! `crate::ui::show` plus the action application and every overlay), so any
//! panic, freeze, or state leak between the Status, Channels, and Communities
//! pages reproduces exactly as in the running app.

use crate::app::App;
use crate::model::{Action, Chat, Content, Delivery, Message, Page, STATUS_BROADCAST_ID};
use crate::paths::AppDirs;
use crate::settings::Settings;

fn app() -> (App, egui::Context) {
    let root = std::env::temp_dir().join(format!(
        "wavo-pages-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let (mut app, _events) = App::headless(AppDirs::under(&root), Settings::default());
    let ctx = egui::Context::default();
    app.attach(&ctx);
    (app, ctx)
}

fn story_message(author: &str, id: &str, timestamp: i64) -> Message {
    Message {
        id: id.into(),
        chat: STATUS_BROADCAST_ID.into(),
        sender: author.into(),
        sender_name: Some("Author".into()),
        from_me: false,
        timestamp,
        content: Content::text("status update"),
        status: Delivery::None,
        delivered_at: None,
        read_at: None,
        quoted: None,
        reactions: Vec::new(),
        edited: false,
        mentions: Vec::new(),
        forwarded: false,
        thumbnail: None,
    }
}

/// One community parent, its subgroup, a channel, a direct chat, and the
/// status round-up with one author's two updates.
fn seeded() -> (App, egui::Context) {
    let (mut app, ctx) = app();
    let mut parent = Chat::new("1203999000001@g.us".into(), "Hood".into());
    parent.community = Some("1203999000001@g.us".into());
    let mut subgroup = Chat::new("1203999000002@g.us".into(), "Block party".into());
    subgroup.community = Some("1203999000001@g.us".into());
    subgroup.unread = 2;
    parent.participants = vec!["555001@s.whatsapp.net".into()];
    subgroup.participants = vec!["555001@s.whatsapp.net".into()];
    app.chats = vec![subgroup];
    app.chats.extend([
        Chat::new("555001@s.whatsapp.net".into(), "Ada".into()),
        Chat::new("18008885555@newsletter".into(), "The Daily Bugle".into()),
        parent,
        Chat::new(STATUS_BROADCAST_ID.into(), "Status".into()),
    ]);
    let conversation = app
        .conversations
        .entry(STATUS_BROADCAST_ID.into())
        .or_default();
    conversation.complete = true;
    conversation
        .messages
        .push(story_message("555001@s.whatsapp.net", "s1", 100));
    conversation
        .messages
        .push(story_message("555001@s.whatsapp.net", "s2", 101));
    (app, ctx)
}

fn drive_frames(app: &mut App, ctx: &egui::Context, frames: usize) {
    for _ in 0..frames {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1180.0, 780.0),
                )),
                ..Default::default()
            },
            |ui| app.frame_ui(ui),
        );
        output.textures_delta.clear();
    }
}

#[test]
fn opening_pages_story_and_chats_survives_many_frames() {
    let (mut app, ctx) = seeded();
    drive_frames(&mut app, &ctx, 3);

    for page in [Page::Chats, Page::Status, Page::Channels, Page::Communities] {
        app.actions.push(Action::Open(page.clone()));
        drive_frames(&mut app, &ctx, 5);
        assert_eq!(app.page, page, "the page switch must stick");
    }

    // Story: open, page through, close — no panic, no freeze, state resets.
    app.actions.push(Action::Open(Page::Status));
    app.open_story("555001@s.whatsapp.net".into());
    drive_frames(&mut app, &ctx, 10);
    assert!(
        app.status_story.is_some(),
        "the story stays open while drawing its messages"
    );
    app.actions.push(Action::StoryNext);
    app.actions.push(Action::StoryNext); // past the edge closes the story
    drive_frames(&mut app, &ctx, 10);
    assert!(
        app.status_story.is_none(),
        "stepping past the last update closes the story"
    );
    assert!(
        app.status_seen_at.is_some(),
        "a played story is marked seen"
    );
}

#[test]
fn opening_a_community_or_channel_conversation_keeps_every_tab_working() {
    let (mut app, ctx) = seeded();
    drive_frames(&mut app, &ctx, 3);

    // Open a community subgroup from the Communities roster.
    app.actions
        .push(Action::OpenChat("1203999000002@g.us".into()));
    drive_frames(&mut app, &ctx, 10);
    assert_eq!(
        app.open_chat.as_deref(),
        Some("1203999000002@g.us"),
        "the community subgroup opens"
    );

    // Switching tabs with the conversation open must not hide pages.
    for page in [Page::Status, Page::Channels, Page::Communities, Page::Chats] {
        app.actions.push(Action::Open(page.clone()));
        drive_frames(&mut app, &ctx, 5);
        assert_eq!(app.page, page, "tab switching works for every tab");
    }

    // Back to Communities, close the conversation like the back arrow does.
    app.actions.push(Action::Open(Page::Communities));
    app.actions.push(Action::CloseChat);
    drive_frames(&mut app, &ctx, 5);
    assert!(app.open_chat.is_none(), "closing clears the conversation");
    // Everything still navigable afterwards.
    app.actions.push(Action::Open(Page::Status));
    drive_frames(&mut app, &ctx, 5);
    assert_eq!(app.page, Page::Status);

    // With a conversation open, the roster pages keep their lists beside it,
    // and clicking a status entry still opens the story over that overlay.
    app.actions
        .push(Action::OpenChat("1203999000002@g.us".into()));
    drive_frames(&mut app, &ctx, 5);
    app.actions.push(Action::Open(Page::Status));
    app.open_story("555001@s.whatsapp.net".into());
    drive_frames(&mut app, &ctx, 5);
    assert!(
        app.status_story.is_some(),
        "a story opens over a stale chat"
    );
    app.actions.push(Action::CloseStory);
    drive_frames(&mut app, &ctx, 5);
    assert!(app.status_story.is_none());
}

#[test]
fn community_parent_channel_and_subgroup_open_in_sequence() {
    let (mut app, ctx) = seeded();

    // Clicking the community parent, the subgroup, and the channel in turn.
    for id in [
        "1203999000001@g.us",
        "1203999000002@g.us",
        "18008885555@newsletter",
    ] {
        app.actions.push(Action::OpenChat(id.into()));
        drive_frames(&mut app, &ctx, 5);
        assert_eq!(app.open_chat.as_deref(), Some(id), "{id} opens");
    }
    // Keep drawing with keyboard navigation paths exercised too.
    drive_frames(&mut app, &ctx, 20);
}

#[test]
fn restored_channel_conversation_recovers_on_the_channels_page() {
    // Occurs when the persisted last chat is a channel after a crash or quit.
    let (mut app, ctx) = seeded();
    app.open_chat = Some("18008885555@newsletter".into());
    app.page = Page::Channels;
    drive_frames(&mut app, &ctx, 5);
    assert_eq!(app.open_chat.as_deref(), Some("18008885555@newsletter"));
    // Escape closes the chat from the Channels page.
    app.actions.push(Action::CloseChat);
    drive_frames(&mut app, &ctx, 5);
    assert!(app.open_chat.is_none());
    // Every page remains reachable afterwards.
    for page in [Page::Chats, Page::Communities, Page::Status, Page::Channels] {
        app.actions.push(Action::Open(page.clone()));
        drive_frames(&mut app, &ctx, 5);
        assert_eq!(app.page, page);
    }
}

#[test]
fn a_story_whose_messages_vanish_closes_itself() {
    let (mut app, ctx) = seeded();
    app.actions.push(Action::Open(Page::Status));
    drive_frames(&mut app, &ctx, 3);

    app.open_story("555001@s.whatsapp.net".into());
    drive_frames(&mut app, &ctx, 5);
    assert!(app.status_story.is_some());
    // When the status round-up reloads mid-play, the story must close itself
    // without panicking or freezing the frame loop.
    app.conversations.remove(STATUS_BROADCAST_ID);
    drive_frames(&mut app, &ctx, 5);
    assert!(
        app.status_story.is_none(),
        "a story with no messages closes"
    );
}
