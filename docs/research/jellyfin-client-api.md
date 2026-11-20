# Research: what the Jellyfin API needs from a Korri client (#561)

Ticket: #561, under map #559. Date: 2026-11-20. Research only: no code was changed.

Labels: **verified** = I read the spec or the source line; **inferred** = follows from what I read, not proven; **opinion** = my judgement. I did not run a Jellyfin server, so no claim here is measured by me. Claims marked "measured by mpv-shim" come from that project's notes, not from my own test.

## Sources and versions

| Source | Version read | What it is |
|---|---|---|
| [Jellyfin OpenAPI stable spec](https://api.jellyfin.org/openapi/jellyfin-openapi-stable.json) | `x-jellyfin-version` 12.4.0, 294 paths | The official API spec. Rendered as ReDoc at [api.jellyfin.org](https://api.jellyfin.org/). |
| [jellyfin/jellyfin](https://github.com/jellyfin/jellyfin) server source | tag `v12.4` (released 2026-10-26), commit `83d0d0e7f76e` | The server. Used where the spec is silent or incomplete. |
| [jellyfin/jellyfin-mpv-shim](https://github.com/jellyfin/jellyfin-mpv-shim) | commit `86438d1ff5a9` (2026-11-19) | A native Linux client (Python + mpv). Its `docs/jellyfin-api-notes.md` records server behaviour measured against 10.11 and 12.0. |
| [jellyfin/jellyfin-apiclient-python](https://github.com/jellyfin/jellyfin-apiclient-python) | commit `d6a33783f926` (2026-11-15) | The API client that mpv-shim uses. |
| [Jellyfin docs: Codec support](https://jellyfin.org/docs/general/clients/codec-support/), [Transcoding](https://jellyfin.org/docs/general/post-install/transcoding/), [Quick Connect](https://jellyfin.org/docs/general/server/quick-connect/) | fetched 2026-11-20 | Official user docs. They name the play methods. |

Note: the current Jellyfin server is version 12.x. The previous stable spec is 12.3.0. mpv-shim targets "10.11 and 12.0".

## Answer

A Korri client needs one HTTP header, about ten endpoints and one JSON document (the `DeviceProfile`):

1. **Sign in** with `POST /Users/AuthenticateByName` (user name and password) or with Quick Connect. Keep the `AccessToken` it returns. Send it on every request in `Authorization: MediaBrowser Client="…", Device="…", DeviceId="…", Version="…", Token="…"`.
2. **List movies** with `GET /Items?IncludeItemTypes=Movie&Recursive=true&Fields=…`. Art comes from `GET /Items/{id}/Images/{type}` with the tag from `ImageTags`.
3. **Play** with `POST /Items/{id}/PlaybackInfo` and a `DeviceProfile` that says what the local player can decode. The server answers with `MediaSources[]`. If `SupportsDirectPlay` is true, the client streams the original file from `GET /Videos/{id}/stream?static=true&MediaSourceId=…`. If not, the client plays the server's `TranscodingUrl` (HLS).
4. **Report** with `POST /Sessions/Playing` at start, `POST /Sessions/Playing/Progress` while playing, and `POST /Sessions/Playing/Stopped` at the end. The server turns the stop position into `UserData.PlaybackPositionTicks` and `UserData.Played` itself.

Time is in ticks everywhere: 10,000,000 ticks per second.

## 1. Sign-in and tokens

### 1.1 The `Authorization` header (verified)

The spec declares one security scheme, `CustomAuthentication`: an API key in the `Authorization` header. The spec does not define the format. The server source does (`Jellyfin.Server.Implementations/Security/AuthorizationContext.cs`):

- The scheme name must be `MediaBrowser`. `Emby` is accepted only when the server option `EnableLegacyAuthorization` is on.
- The value is a comma-separated list of `Key="value"` pairs. The server reads `DeviceId`, `Device`, `Client`, `Version` and `Token`. Values are URL-decoded.
- If the header has no token, the server reads the `ApiKey` query parameter. The headers `X-Emby-Token`, `X-MediaBrowser-Token`, `X-Emby-Authorization` and the query parameter `api_key` work only when `EnableLegacyAuthorization` is on.
- `EnableLegacyAuthorization` has no initialiser in `MediaBrowser.Model/Configuration/ServerConfiguration.cs`, so it is `false` by default (verified). mpv-shim's notes say the same: "off by default from Jellyfin v12".

Consequence: send `Authorization: MediaBrowser …` on every request. Where a player cannot send headers, use `?ApiKey=`, never `api_key`.

The device fields matter. The server stores the token against the device (`DeviceId`) and shows `Device`, `Client` and `Version` in its dashboard. A new `DeviceId` makes a new session and a new token on the next sign-in (inferred from `AuthorizationContext` and the devices table it queries). So each Korri device needs a stable `DeviceId`.

### 1.2 User sign-in (verified)

| Step | Call | Body / result |
|---|---|---|
| Password | `POST /Users/AuthenticateByName` | Body `AuthenticateUserByName`: `Username`, `Pw` (plain text). Result `AuthenticationResult`: `User` (`UserDto`), `SessionInfo`, `AccessToken`, `ServerId`. |
| Who am I | `GET /Users/Me` | The signed-in `UserDto`. |
| Server identity | `GET /System/Info/Public` | No auth needed. Server id, name and version. |

The sign-in call has no security requirement in the spec, but it still needs the `Authorization` header with the four device fields and no token, so the server can bind the new token to a device (inferred from `AuthorizationContext`; mpv-shim's `http.py` sends it this way).

### 1.3 Quick Connect (verified)

Quick Connect lets a person sign in a device without typing a password: the device shows a code, and the person approves it from a client where they are already signed in ([Jellyfin docs](https://jellyfin.org/docs/general/server/quick-connect/)). It fits a Korri device with only a controller (opinion).

| Step | Call | Notes |
|---|---|---|
| 1 | `GET /QuickConnect/Enabled` | Returns a boolean. The server option `QuickConnectAvailable` defaults to `true`. |
| 2 | `POST /QuickConnect/Initiate` | Send the device header. Result `QuickConnectResult`: `Secret`, `Code`, `Authenticated`, device and app fields. Show `Code` on screen. Keep `Secret` private. |
| 3 | `GET /QuickConnect/Connect?secret=…` | Poll until `Authenticated` is `true`. |
| 4 | `POST /Users/AuthenticateWithQuickConnect` | Body `QuickConnectDto`: `{ "Secret": … }`. Result: the same `AuthenticationResult` as a password sign-in. |

The approving client calls `POST /QuickConnect/Authorize` with the code. Korri does not need that call unless it also approves other devices.

### 1.4 API keys (verified)

`POST /Auth/Keys?app=…` creates an API key and needs an administrator (`RequiresElevation`). The server treats an API key as an administrator with no user: `CustomAuthenticationHandler.cs` gives `IsApiKey` the `Administrator` role, and `AuthorizationContext` sets no user for it. So with an API key:

- Every per-user call (`/Items`, `/UserItems/…`, `PlaybackInfo`) needs an explicit `userId`. The `/Items` spec says: "this is required when not using an API key".
- Watched state and progress are written for whatever user the client names (inferred).

Opinion: use a user token from sign-in or Quick Connect, not an API key. An API key is a server-wide administrator secret, and Korri would then have to choose the user itself. This choice belongs to the open map question about person and account mapping.

## 2. Listing movies with art and metadata

### 2.1 The query (verified)

`GET /Items` is the general item query. For a movie grid:

```
GET /Items?IncludeItemTypes=Movie&Recursive=true
          &Fields=Overview,Genres,ProviderIds,PrimaryImageAspectRatio,MediaSourceCount
          &EnableImageTypes=Primary,Backdrop,Logo,Thumb&ImageTypeLimit=1
          &SortBy=SortName&SortOrder=Ascending&StartIndex=0&Limit=100
          &EnableTotalRecordCount=true
```

- `ParentId` limits the query to one library. `GET /UserViews` lists the libraries; a movie library has `CollectionType: movies` (inferred from mpv-shim notes §14).
- Paging is `StartIndex` and `Limit`. `TotalRecordCount` comes back when asked for.
- Other useful filters in the spec: `IsPlayed`, `IsFavorite`, `SearchTerm`, `Genres`, `Years`, `SortBy` (for example `DateCreated`, `PremiereDate`, `CommunityRating`).
- `GET /UserItems/Resume?MediaTypes=Video` gives "continue watching". `GET /Items/Latest` gives recently added.

### 2.2 What a `BaseItemDto` carries (verified)

`BaseItemDto` has 155 properties. Fields a movie tile and a detail page use: `Id`, `Name`, `Type`, `ServerId`, `ProductionYear`, `PremiereDate`, `RunTimeTicks`, `OfficialRating`, `CommunityRating`, `Overview`, `Genres`, `ProviderIds`, `ImageTags`, `BackdropImageTags`, `ImageBlurHashes`, `UserData`, `MediaType`, `Container`.

Some fields come back only when named in `Fields` (enum `ItemFields`: `Overview`, `Genres`, `MediaSources`, `MediaStreams`, `People`, `ProviderIds`, `Chapters`, `Trickplay` and others). mpv-shim measured these rules (from its notes §1, §4, §5; not checked by me):

- The server drops an unknown parameter name or an unknown enum value and still answers 200. A typo looks like a working query.
- `ProductionYear`, `RunTimeTicks` and the ratings come back without `Fields`.
- `UserData` is not an `ItemFields` value. It comes back by default (`EnableUserData`).
- Do not ask for `MediaSources` in a list query. It makes the server resolve every file; on one 200-item query it cost about 25 s.
- `EnableImageTypes` is a whitelist: a type not named has no tag in `ImageTags`. `ImageTypeLimit=1` stops 5 to 10 backdrop tags per item.

### 2.3 Images (verified)

`GET /Items/{itemId}/Images/{imageType}` with `imageType` in `Primary`, `Backdrop`, `Logo`, `Thumb`, `Banner`, `Art`, `Disc` and others (enum `ImageType`). Useful query parameters: `maxWidth`, `maxHeight`, `quality`, `tag`, `format`. Send `tag` = the value from `ImageTags` so the server sends strong cache headers. Backdrops are numbered: `/Images/Backdrop/{imageIndex}`.

mpv-shim measured that the server caches each exact pixel size, so a client should ask for a small fixed set of widths (notes §5; not checked by me).

## 3. Playing a movie

### 3.1 Three play methods (verified)

The enum `PlayMethod` is `DirectPlay`, `DirectStream`, `Transcode`. The Jellyfin docs name four outcomes ([Transcoding](https://jellyfin.org/docs/general/post-install/transcoding/)):

| Outcome | What the server does | Server CPU |
|---|---|---|
| Direct Play | Sends the original file unchanged. | None |
| Remux | Changes only the container. Audio and video are copied. | Low |
| Direct Stream | Copies the video, re-encodes the audio. | Low |
| Transcode | Re-encodes the video. | High |

The [codec support page](https://jellyfin.org/docs/general/clients/codec-support/) says "The goal is to Direct Play all media", and that subtitles can force a remux or a burn-in transcode.

### 3.2 `PlaybackInfo` decides (verified)

`POST /Items/{itemId}/PlaybackInfo` with body `PlaybackInfoDto`:

| Field | Use |
|---|---|
| `UserId` | Needed only with an API key. |
| `DeviceProfile` | What the player can decode. See §3.3. |
| `MaxStreamingBitrate` | Upper limit in bits per second. |
| `StartTimeTicks` | Resume position. A transcode starts there. |
| `MediaSourceId`, `AudioStreamIndex`, `SubtitleStreamIndex` | Track choice. mpv-shim measured that the server ignores `AudioStreamIndex` unless `MediaSourceId` is also sent (notes §11). |
| `EnableDirectPlay`, `EnableDirectStream`, `EnableTranscoding`, `AllowVideoStreamCopy`, `AllowAudioStreamCopy` | Switch paths off, for example to force a transcode after a direct play fails. |

The query-string form of these parameters is marked obsolete in the spec; send the body.

Result `PlaybackInfoResponse`: `MediaSources[]` (`MediaSourceInfo`), `PlaySessionId`, `ErrorCode`. Each source has `Id`, `Container`, `Size`, `Bitrate`, `RunTimeTicks`, `MediaStreams[]` (each with `Type`, `Codec`, `Profile`, `Level`, `BitDepth`, `Width`, `Height`, `Channels`, `VideoRangeType`, `Language`, `IsDefault`, `DeliveryMethod`, `DeliveryUrl`), `SupportsDirectPlay`, `SupportsDirectStream`, `SupportsTranscoding`, `TranscodingUrl`, `TranscodingSubProtocol`, `TranscodingContainer`, `DefaultAudioStreamIndex`, `DefaultSubtitleStreamIndex`.

What the server does with it (`Jellyfin.Api/Helpers/MediaInfoHelper.cs`, verified):

- `StreamBuilder.GetOptimalVideoStream` compares each stream with the profile and picks a `PlayMethod`.
- **"direct-stream http streaming is currently broken"** (comment in the source). Unless `ForceDirectStream` is set, the server sets `EnableDirectStream = false`. Nothing outside tests sets `ForceDirectStream`. So in 12.4 a remux or a direct stream reaches the client as a `TranscodingUrl` that copies the streams, not as a separate URL (verified for the code path; inferred for every case).
- When the source is not direct-playable and can be converted, the server sets `PlayMethod = Transcode` and fills `TranscodingUrl`. `StreamInfo.ToUrl` builds it as `/videos/{id}/master.m3u8?…` for HLS or `/videos/{id}/stream.{container}?…` for HTTP, and it appends `ApiKey=<token>`.
- User policy can block paths: `EnableMediaPlayback`, `EnablePlaybackRemuxing`, `EnableVideoPlaybackTranscoding`, `EnableAudioPlaybackTranscoding`, and `ForceRemoteSourceTranscoding`.

### 3.3 The `DeviceProfile` (verified)

The `DeviceProfile` is how the client says which codecs and containers it plays. The server, not the client, chooses the play method from it.

| Part | Type | Meaning |
|---|---|---|
| `MaxStreamingBitrate`, `MaxStaticBitrate` | integer | Bit-rate caps for transcodes and for direct play. |
| `DirectPlayProfiles[]` | `Container`, `VideoCodec`, `AudioCodec`, `Type` | Combinations the player opens as they are. Values are comma lists. |
| `TranscodingProfiles[]` | `Container`, `Type`, `VideoCodec`, `AudioCodec`, `Protocol` (`http` or `hls`), `Context` (`Streaming` or `Static`), `MaxAudioChannels`, `Conditions[]` … | What to convert to when direct play fails. |
| `CodecProfiles[]` | `Type` (`Video`, `VideoAudio`, `Audio`), `Codec`, `Container`, `Conditions[]`, `ApplyConditions[]` | Limits inside a codec, for example `VideoBitDepth LessThanEqual 8`. |
| `ContainerProfiles[]` | conditions | "Failing to meet these optional conditions causes transcoding to occur." |
| `SubtitleProfiles[]` | `Format`, `Method` (`Encode`, `Embed`, `External`, `Hls`, `Drop`), `Container` | How each subtitle format may arrive. `Encode` means burned in, which forces a video transcode. |

A condition is `{ Condition, Property, Value, IsRequired }`. `Condition` is one of `Equals`, `NotEquals`, `LessThanEqual`, `GreaterThanEqual`, `EqualsAny`. `Property` is one of 26 values, including `Width`, `Height`, `VideoBitDepth`, `VideoLevel`, `VideoProfile`, `VideoRangeType`, `VideoFramerate`, `AudioChannels`, `IsInterlaced`.

The enum `TranscodeReason` (28 values, for example `ContainerNotSupported`, `VideoCodecNotSupported`, `AudioCodecNotSupported`, `SubtitleCodecNotSupported`, `VideoRangeTypeNotSupported`, `ContainerBitrateExceedsLimit`) tells why direct play was refused. mpv-shim measured that the reasons arrive as a `TranscodeReasons` query parameter of `TranscodingUrl`, not as a DTO field on 10.11 (notes §11).

**Which codecs each path needs.** Jellyfin sets no fixed list. Each path needs what the client declares:

- Direct play needs the player to open the file's container and decode its video, audio and chosen subtitle as they are.
- Remux and direct stream need the player to decode the video codec. The container and audio are what the `TranscodingProfile` names.
- Transcode needs only what the `TranscodingProfile` names. mpv-shim asks for HLS in `ts` with video `h264,mpeg4,mpeg2video` (plus `hevc` by option) and audio `aac,mp3,ac3,opus,flac,vorbis` (`jellyfin_mpv_shim/utils.py`, `get_profile`).

mpv-shim's profile is the reference for an ffmpeg-based player (verified in its source): `DirectPlayProfiles` is `[{Type: Video}, {Type: Audio}, {Type: Photo}]` with no container or codec, which means "play anything". Its notes say an empty `Container` in a `DirectPlayProfile` means "any" to the server (`ContainerHelper.ContainsContainer`; not checked by me). It lists text and image subtitle formats as `External` or `Embed`, so the server never burns them in.

Consequence for Korri (inferred): Korri's profile must come from what the local player really decodes on that device. A device with hardware decode for H.264 and HEVC only must not declare AV1, or playback fails instead of transcoding. If the player is ffmpeg-based (as mpv is), the "play anything" profile is the simplest one, but it puts the risk of a slow software decode on the device. This links to the open question of which player Korri uses (see "Questions for the map").

### 3.4 Stream URLs (verified)

| Path | URL |
|---|---|
| Direct play | `GET /Videos/{itemId}/stream?static=true&MediaSourceId={id}` (or `/stream.{container}`). `static`: "the original file will be streamed statically without any encoding". It supports HTTP range requests (inferred: the server returns a file result). |
| Transcode, remux, direct stream | `{server}{TranscodingUrl}` exactly as returned. Usually `/videos/{id}/master.m3u8?…&PlaySessionId=…&ApiKey=…`. |
| Download the original | `GET /Items/{itemId}/Download`. Needs the `Download` policy (`EnableContentDownloading`). Relevant to the offline-download question. |
| Subtitles as a separate file | `MediaStream.DeliveryUrl` when `DeliveryMethod` is `External`. |
| Stop a transcode | `DELETE /Videos/ActiveEncodings?DeviceId=…&PlaySessionId=…`. mpv-shim calls this when it leaves a transcoded item. |

Two gaps in the spec:

- **The HLS routes are not in the OpenAPI spec.** `DynamicHlsController` has `[ApiExplorerSettings(IgnoreApi = true)]`, so `/Videos/{id}/master.m3u8` and `/main.m3u8` exist in the server but are absent from all four published spec files (verified). A client generated from the spec has no HLS method. It should use `TranscodingUrl` as an opaque string.
- `/Videos/{id}/stream` has no security requirement in the spec, and `VideosController` has no `[Authorize]` on that action (verified). Opinion: send the token anyway, in the header or as `ApiKey`, and do not depend on the open route.

The player must reach the URL with the token. mpv-shim gives mpv the `Authorization` header, or adds `ApiKey` when it cannot (`docs/auth-headers.md`). It also refuses to set the global header when a subtitle comes from another host, so the token does not leak there.

## 4. Progress and watched state

### 4.1 Session reports (verified)

| Event | Call | Body |
|---|---|---|
| Start | `POST /Sessions/Playing` | `PlaybackStartInfo` |
| Every few seconds, and on pause, seek or track change | `POST /Sessions/Playing/Progress` | `PlaybackProgressInfo` |
| End | `POST /Sessions/Playing/Stopped` | `PlaybackStopInfo` |
| Keep alive without progress | `POST /Sessions/Playing/Ping?playSessionId=…` | none |

Start and progress bodies carry `ItemId`, `MediaSourceId`, `PlaySessionId` (from `PlaybackInfo`), `PositionTicks`, `IsPaused`, `IsMuted`, `VolumeLevel`, `AudioStreamIndex`, `SubtitleStreamIndex`, `PlayMethod`, `CanSeek`, `PlaybackStartTimeTicks`. The stop body carries `ItemId`, `MediaSourceId`, `PlaySessionId`, `PositionTicks`, `Failed`. All return 204.

mpv-shim sends start once, then progress from a timeline thread that wakes every 5 s or on an event (`timeline.py`, `trigger.wait(5)`), then stop. It sends progress only after start has completed, so progress cannot arrive first (`player_reporting.py`). It reports `PlayMethod` as `Transcode` or `DirectPlay` only.

The server rejects a negative stop `PositionTicks` (`SessionManager.OnPlaybackStopped`). If a session sends nothing for more than 5 minutes while an item plays, the server stops it itself at the last reported position (`CheckForIdlePlayback`, every 5 minutes). So a crash loses at most the progress since the last report (inferred).

### 4.2 How the server turns a position into watched state (verified)

`UserDataManager.UpdatePlayState` runs on progress and on stop. For a video with a known runtime, with the defaults `MinResumePct` 5, `MaxResumePct` 90, `MinResumeDurationSeconds` 300:

| Position | Result |
|---|---|
| Under 5 % | Position stored as 0. Not played. |
| Over 90 %, or within 1 s of the end | `Played = true`, position 0. |
| Between, and the item is shorter than 300 s | `Played = true`, position 0. |
| Between, otherwise | `PlaybackPositionTicks` = the position. This is the resume point. |
| Runtime unknown | `Played = true`. |

Audiobooks use minute thresholds (`MinAudiobookResume`, `MaxAudiobookResume`) instead. Books are excluded from both arms. This matters for the audiobook part of the map.

So a client does not mark a movie watched itself. It reports the true stop position and the server decides.

### 4.3 `UserData` (verified)

`UserItemDataDto`: `PlaybackPositionTicks`, `PlayedPercentage`, `Played`, `PlayCount`, `LastPlayedDate`, `IsFavorite`, `Likes`, `Rating`, `UnplayedItemCount`, `Key`, `ItemId`. It comes on every item in a list query.

| Action | Call |
|---|---|
| Mark played | `POST /UserPlayedItems/{itemId}` (optional `datePlayed`) |
| Mark unplayed | `DELETE /UserPlayedItems/{itemId}` |
| Write user data directly | `POST /UserItems/{itemId}/UserData` with `UpdateUserItemDataDto` |
| Read | `GET /UserItems/{itemId}/UserData` |

mpv-shim measured (notes §10; not checked by me): mark played clears the resume position; `POST /UserItems/{id}/UserData` stores the position as given and skips the 5 %/90 % rules; a progress report sends no `UserDataChanged` websocket event, only start, stop and explicit marks do.

## 5. TV shows, seasons and episodes (for the grouping ticket)

The kinds are `Series`, `Season` and `Episode` in the enum `BaseItemKind` (verified).

| Level | How to list | Key fields |
|---|---|---|
| Series | `GET /Items?IncludeItemTypes=Series&Recursive=true` | `Id`, `Name`, `ProductionYear`, `Status`, `UserData.UnplayedItemCount`, `ChildCount` (when asked) |
| Season | `GET /Shows/{seriesId}/Seasons` (filter `isSpecialSeason`, `isMissing`) | `Id`, `IndexNumber` (season number; 0 is specials), `SeriesId`, `ParentBackdropItemId`, `ParentBackdropImageTags` |
| Episode | `GET /Shows/{seriesId}/Episodes?seasonId=…` (or `season=` number; `startItemId`, paging) | `SeriesId`, `SeriesName`, `SeasonId`, `SeasonName`, `ParentIndexNumber` (season number), `IndexNumber` (episode number), `IndexNumberEnd` (multi-episode file), `RunTimeTicks`, `UserData`, `SeriesPrimaryImageTag` |
| Next to watch | `GET /Shows/NextUp?seriesId=…` (`enableResumable`, `enableRewatching`) | Episodes |

The server fills `SeriesId` and `SeasonId` on episodes and `SeriesId` on seasons (`DtoService.cs`). Only an episode is playable; a series or a season is a folder. Episodes play and report exactly like movies (inferred: the same `PlaybackInfo` and session calls apply to any video item). A season or series can be marked played as a whole with `POST /UserPlayedItems/{id}`. mpv-shim measured that a per-item read after such a mark can be stale, while `GET /Items?Ids=` is correct (notes §10; not checked by me).

## Contradictions and limits

- The spec has no HLS routes and does not describe the `Authorization` header format. Both come from the server source.
- The spec field `MediaSourceInfo.SupportsDirectStream` exists, but the 12.4 server sets direct stream off for HTTP. Do not build a separate direct-stream URL path on it.
- I did not run a server. The behaviour notes from mpv-shim are measured by that project, on 10.11 and 12.0, not on 12.4.
- I did not check other native clients (Jellyfin Media Player, Swiftfin, Findroid). mpv-shim is the closest shape to Korri: a Linux native client with an external ffmpeg-based player.

## Questions for the map

1. **Which player does Korri use for media on a device?** The `DeviceProfile` must match it exactly. mpv (libmpv) is what mpv-shim uses; the game-stream viewer is the only decoder on main today (map note).
2. **How is the `DeviceProfile` produced per device?** Fixed per device, generated from probed decoders, or declared by the plugin. This touches the federation rule: the device that plays has the capability.
3. **Which device signs in, and where does the token live?** The token is per `DeviceId`. One token per Korri device, or one per federation? This joins the open question on person and account mapping.
4. **Who sends the progress reports?** The device that plays has the position; korrid on that device performs the HTTP effect. The plugin should only declare.

## Sources

Answer: sign-in and header

- [Jellyfin OpenAPI stable spec 12.4.0](https://api.jellyfin.org/openapi/jellyfin-openapi-stable.json) (Jellyfin) — `CustomAuthentication`, `/Users/AuthenticateByName`, Quick Connect routes, `/Auth/Keys`. checked
- [`AuthorizationContext.cs` at v12.4](https://github.com/jellyfin/jellyfin/blob/v12.4/Jellyfin.Server.Implementations/Security/AuthorizationContext.cs) (Jellyfin) — `MediaBrowser` scheme, fields, `ApiKey`, legacy gates. checked
- [`ServerConfiguration.cs` at v12.4](https://github.com/jellyfin/jellyfin/blob/v12.4/MediaBrowser.Model/Configuration/ServerConfiguration.cs) (Jellyfin) — `EnableLegacyAuthorization`, `QuickConnectAvailable`, resume thresholds. checked
- [`CustomAuthenticationHandler.cs` at v12.4](https://github.com/jellyfin/jellyfin/blob/v12.4/Jellyfin.Api/Auth/CustomAuthenticationHandler.cs) (Jellyfin) — API key gets the administrator role. checked
- [Quick Connect](https://jellyfin.org/docs/general/server/quick-connect/) (Jellyfin docs) — what Quick Connect is for. checked
- [`http.py`](https://github.com/jellyfin/jellyfin-apiclient-python/blob/master/jellyfin_apiclient_python/http.py) and [`api.py`](https://github.com/jellyfin/jellyfin-apiclient-python/blob/master/jellyfin_apiclient_python/api.py) (jellyfin-apiclient-python) — header format and Quick Connect flow as a client sends them. checked

Answer: playback

- OpenAPI spec (above) — `PlaybackInfoDto`, `PlaybackInfoResponse`, `MediaSourceInfo`, `DeviceProfile` and its parts, `PlayMethod`, `TranscodeReason`, `/Videos/{id}/stream`. checked
- [`MediaInfoHelper.cs` at v12.4](https://github.com/jellyfin/jellyfin/blob/v12.4/Jellyfin.Api/Helpers/MediaInfoHelper.cs) (Jellyfin) — direct stream off, `TranscodingUrl`, user policy. checked
- [`StreamInfo.cs` at v12.4](https://github.com/jellyfin/jellyfin/blob/v12.4/MediaBrowser.Model/Dlna/StreamInfo.cs) (Jellyfin) — URL shape `master.m3u8` / `stream.{container}`, `ApiKey`. checked
- [`DynamicHlsController.cs` at v12.4](https://github.com/jellyfin/jellyfin/blob/v12.4/Jellyfin.Api/Controllers/DynamicHlsController.cs) (Jellyfin) — `IgnoreApi = true`, HLS routes. checked
- [`VideosController.cs` at v12.4](https://github.com/jellyfin/jellyfin/blob/v12.4/Jellyfin.Api/Controllers/VideosController.cs) (Jellyfin) — stream action without `[Authorize]`. checked
- [Transcoding](https://jellyfin.org/docs/general/post-install/transcoding/) and [Codec support](https://jellyfin.org/docs/general/clients/codec-support/) (Jellyfin docs) — play-method definitions. checked
- [`media.py`](https://github.com/jellyfin/jellyfin-mpv-shim/blob/master/jellyfin_mpv_shim/media.py), [`utils.py`](https://github.com/jellyfin/jellyfin-mpv-shim/blob/master/jellyfin_mpv_shim/utils.py), [`docs/auth-headers.md`](https://github.com/jellyfin/jellyfin-mpv-shim/blob/master/docs/auth-headers.md) (jellyfin-mpv-shim) — URL choice, device profile, token handling. checked

Answer: progress and watched state

- OpenAPI spec (above) — `/Sessions/Playing*`, `PlaybackStartInfo`, `PlaybackProgressInfo`, `PlaybackStopInfo`, `UserItemDataDto`, `/UserPlayedItems`, `/UserItems/{id}/UserData`. checked
- [`UserDataManager.cs` at v12.4](https://github.com/jellyfin/jellyfin/blob/v12.4/Emby.Server.Implementations/Library/UserDataManager.cs) (Jellyfin) — `UpdatePlayState` thresholds. checked
- [`SessionManager.cs` at v12.4](https://github.com/jellyfin/jellyfin/blob/v12.4/Emby.Server.Implementations/Session/SessionManager.cs) (Jellyfin) — idle stop after 5 minutes, negative position check. checked
- [`timeline.py`](https://github.com/jellyfin/jellyfin-mpv-shim/blob/master/jellyfin_mpv_shim/timeline.py) and [`player_reporting.py`](https://github.com/jellyfin/jellyfin-mpv-shim/blob/master/jellyfin_mpv_shim/player_reporting.py) (jellyfin-mpv-shim) — 5 s cadence, report body. checked

Other

- [`docs/jellyfin-api-notes.md`](https://github.com/jellyfin/jellyfin-mpv-shim/blob/master/docs/jellyfin-api-notes.md) (jellyfin-mpv-shim) — measured server behaviour: dropped parameters, `Fields` cost, images, userdata events, audio index rule. read; its measurements not repeated by me
- [`DtoService.cs` at v12.4](https://github.com/jellyfin/jellyfin/blob/v12.4/Emby.Server.Implementations/Dto/DtoService.cs) (Jellyfin) — `SeriesId`, `SeasonId` on episodes and seasons. checked
