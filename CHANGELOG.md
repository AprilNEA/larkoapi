# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.5.0] — 2026-04-20

### Added

- **Unofficial Minutes (妙记) web client** behind feature `minutes-unofficial`:
  - `MinutesWebClient::new(base, cookie, http)` — cookie-authenticated client
    with automatic `bv-csrf-token` extraction.
  - `list_page(space, size, cursor)` / `list_all(space, size)` — enumerate
    the Minutes inbox (pagination by `share_time`).
  - `get_media_url(object_token)` — resolve the A/V download URL.
  - `export_subtitle(object_token, opts)` — export transcript as SRT or TXT
    with optional speaker labels and timestamps.
  - `session_expires_at()` / `needs_refresh(buffer)` — decode the
    `sl_session` JWT `exp` claim locally.
  - `reload_cookie(new)` — swap the cookie after a cron re-harvest.
  - `with_security_host(host)` / `refresh()` — trigger Lark's
    compliance-ping heartbeat (`/lark/scs/compliance/ping`), which rotates
    `sl_session` for another 12 hours without re-harvesting.
  - `infer_security_host_from_base(base)` — best-effort host derivation for
    Lark International regional tenants.
  - Constants `FEISHU_BASE`, `LARK_BASE` for the web hosts.
  - Types: `MinutesWebRecord`, `MinutesWebPage`, `SpaceName`,
    `SubtitleFormat`, `SubtitleOptions`.
  - **Stability note:** these endpoints are undocumented and may break
    without notice. Reference implementation:
    <https://github.com/bingsanyu/feishu_minutes>.

### Changed

- `minutes` is now a directory module (`src/minutes/{mod,official,unofficial}.rs`).
  Public types (`MinuteMeta`) remain at the same path, so nothing downstream
  needs to change.

## [0.4.0] — 2026-04-20

### Added

- **VC v1** (`vc` module):
  - `LarkBotClient::get_meeting(meeting_id)` → `MeetingMeta { topic, start_time_ms, end_time_ms, owner_open_id }`
  - `LarkBotClient::get_recording(meeting_id)` → `RecordingFile { url, duration_ms }`
  - Scopes: `vc:meeting:readonly`, `vc:record:readonly`.
- **Minutes v1** (`minutes` module):
  - `LarkBotClient::get_minute_meta(minute_token)` → `MinuteMeta`
  - `LarkBotClient::get_minute_media_url(minute_token)` → signed A/V download URL (1-day TTL)
  - Scopes: `minutes:minutes:readonly`, `minutes:minutes.media:export`.
- Types re-exported at the crate root: `MeetingMeta`, `RecordingFile`, `MinuteMeta`.

### Changed

- Internal `call(method, path, body)` helper is now `pub(crate)` so sibling
  modules can layer typed endpoints without re-implementing auth and error
  handling. No change for external users.

## [0.3.0] — 2026-04-13

> Note: `0.2.0` was published earlier with only the typed-card builders. This
> release bundles the rest of the 0.2.x work (Docx / Drive / IM extensions,
> `models` module, static-TLS build) on top of it.


### Breaking

- **`reqwest` now compiled with `default-features = false`** (only `rustls-tls`,
  `json`, `multipart`). Downstream crates relying on the transitive
  `native-tls`/`default-tls` from this crate will need to enable it explicitly.
  This unblocks static cross-compilation to `*-linux-musl` targets.
- `LarkCard` gained a new `config: Option<CardConfig>` field. Struct-literal
  construction like `LarkCard { header, elements }` stops compiling — use the
  `LarkCard::new(template, title).push(...)` builder instead.

### Added

- **Typed card element builders** in `card.rs`:
  `MdBlock`, `Hr`, `ImageElement`, `NoteElement`, `ActionGroup`, `ColumnSet`,
  `Column`, plus `LarkCard::new` / `push` / `extend` / `shared` fluent API
  and `CardConfig` for shared-card updates.
- **IM extensions** on `LarkBotClient`:
  - `send_text(receive_id, receive_id_type, text)` — plain text messages
  - `send_interactive_returning_id(receive_id, receive_id_type, card)` —
    generic card send that returns the `message_id`
  - `send_card_returning_id(chat_id, card)` — chat-only variant (kept for
    backwards compat with 0.1.1)
  - `update_card(message_id, card)` — PATCH an existing interactive message
  - `upload_image(jpeg)` — returns `image_key`
  - `urgent_app(message_id, open_ids)` — in-app urgent notification
  - `list_chat_members(chat_id)` → `Vec<ChatMember>`
  - `bot_open_id()` — fetch the bot's own `open_id` via `/bot/v3/info`
- **Docx / Drive extensions** on `LarkBotClient`:
  - `create_docx_in_folder(folder_token, title)` → `document_id`
  - `list_document_blocks(document_id)` → `Vec<Value>` (paginated)
  - `insert_document_children(document_id, parent_block_id, index, children)`
  - `batch_update_document_blocks(document_id, requests)`
  - `list_files_in_folder(folder_token)` → `Vec<DriveFile>` (paginated)
  - `share_file_with_chat(file_token, file_type, chat_id)` — grant a chat
    group edit permission on a Drive file
- **New `models` module** exporting `ChatMember` and `DriveFile` response types.
- **WebSocket long-connection client** in `ws.rs`:
  - `WsEventHandler` trait (`async fn handle_event(&self, event: &Value) -> Option<Value>`)
  - `run_ws_client(base_url, app_id, app_secret, handler, http)` —
    reconnecting client that handles ping/pong, message fragmentation,
    dedup, and dispatches `im.message.receive_v1` / `card.action.trigger`
    events to the handler.

### Changed

- Internal `LarkBotClient::call(method, path, body)` helper centralises token
  refresh, bearer auth, HTTP status check, and `code != 0` → `Err` handling.
  New public methods layer on top of this helper instead of duplicating the
  boilerplate that the 0.1 send paths carried.
- `reply_to_chat` now delegates to the generic `send_message(_, "chat_id", _)`.

## [0.1.1] — 2026-04-07

### Added

- `send_message(receive_id, receive_id_type, card)` — generic interactive card
  send for arbitrary receive_id_type.
- `send_dm(email, card)` — DM by email (thin wrapper over `send_message`).
- `reply_to_chat(chat_id, card)` — chat_id wrapper.

## [0.1.0] — 2026-04-06

### Added

- Initial release.
- `LarkBotClient::new(app_id, app_secret, base_url, http)` with tenant
  access-token caching (5-minute refresh buffer).
- `base_url` configurable for Lark international
  (`https://open.larksuite.com`) and Feishu China
  (`https://open.feishu.cn`).

[0.3.0]: https://github.com/AprilNEA/larkoapi/releases/tag/v0.3.0
[0.2.0]: https://github.com/AprilNEA/larkoapi/releases/tag/v0.2.0
[0.1.1]: https://github.com/AprilNEA/larkoapi/releases/tag/v0.1.1
[0.1.0]: https://github.com/AprilNEA/larkoapi/releases/tag/v0.1.0
