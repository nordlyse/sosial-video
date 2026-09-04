# Sosial Video

A social video sharing application. Development happens on [nordlyse/sosial-video](https://github.com/nordlyse/sosial-video).

License: [MIT](LICENSE). Following Criterias.md, internal services use Rust, the web UI uses React, the database is PostgreSQL 17, and frames are drawn with WebGPU.

## Status

A runnable skeleton is in place: sign in and registration with email confirmation, test users, a contact service, and a shared WebRTC distribution service. Friend approval, hand raising, reactions, and comments come later.

Working branch: **develop**. `main` is the stable / release line.

## Purpose

When someone starts a video, their friends should be notified and can connect if they want. When the broadcaster accepts them, they can be admitted as a speaker or as a listener only. If admitted as a speaker, they can raise their hand and share opinions on the video so a discussion can happen. If admitted as a listener only, they cannot speak, but they can listen and can turn on their camera. Both listeners and speakers can send live reactions and comments. For example they can drop like, dislike, angry-face, and similar expressions live; these appear as counts under the video (10 liked, 5 disliked, and so on). Comments also appear under the video.

Even if someone is not a friend, a public stream should appear on a separate public-stream list visible to all connected users. They can join, but only as listeners. If the stream is private, only friends should see it; it must not appear on the public list. On the live view, all public streams are visible to everyone, but private streams must only be visible to friend groups.

Videos, comments, and similar content are kept for 1 year. If the person who started the stream deletes the video, it should still remain on the server for 1 year.

## Services

Services must be built with Docker and usable through Docker Compose.

| Service | Role | Address |
| --- | --- | --- |
| `frontend` | React UI. Sign in or register an account (email confirmation within 1 day). Frames are drawn with WebGPU. | http://localhost:3000 |
| `frontend-db` | PostgreSQL 17. Users, sessions, and IP records. | localhost:5432 |
| `web-contact-service` | Shared Rust service that stores how users find each other's IP addresses. Login, registration, comments, and speech logs go through this service. | http://localhost:8081 |
| `mailpit` | Local inbox for confirmation emails (development). | http://localhost:8025 |
| `webrtc-service` | Shared Rust SFU that distributes all video. Media goes through this service so many viewers do not congest a mesh. | http://localhost:8082 and UDP 40000-40199 |
| `caddy` | HTTPS reverse proxy so phones can use the camera and watch live video. | https://localhost:8443 |

Speech in a live room is transcribed on each device and stored with that person’s username. PostgreSQL keeps the broadcast id, the log file path, and each spoken line. Text files are written under `log/YYYY-MM-DD/broadcast-{id}.txt` on the host. Chrome or Edge over HTTPS is required for speech logging; Safari on iPhone often cannot transcribe.

## How to run

Docker Desktop (or another Docker Engine with Compose v2) must be running.

From the repository root, build the images and start every service in the background:

```bash
docker compose up --build -d
```

The first build compiles the Rust services and can take several minutes. Later starts are faster:

```bash
docker compose up -d
```

Then open http://localhost:3000. You can sign in with a [test user](#test-users), or register an account. New accounts stay inactive until you click the confirmation link in email (valid for 1 day). Locally, open [Mailpit](http://localhost:8025) to read that email. If the day passes without confirmation, the request expires and that account cannot sign in.

On a phone on the same Wi-Fi, open `https://YOUR_LAN_IP:8443` (the computer’s Wi-Fi address, not localhost). Continue past the certificate warning, then allow camera and microphone access if you want to publish. Browsers block the camera on plain `http://` except for localhost, which is why HTTPS on port 8443 is required for live video. If iOS asks, allow Local Network access for the browser. The SFU advertises that same LAN address for WebRTC, so the host computer and the phone can both see each other.

To try a two-person session, open a second browser (or a private window). Have `bob` start a broadcast with a title such as `#discussion`. `alice` can find it under **Public** or **Live broadcasts**, click **Ask to join**, and wait for `bob` to accept her as a listener or speaker. If Alice's camera is on, she appears as a small tile. If she is a speaker and talks, her video becomes the main view for everyone.

Useful commands:

```bash
docker compose ps          # service status
docker compose logs -f     # follow logs
docker compose down        # stop containers
docker compose down -v     # stop and delete the database volume
```

`frontend-db` credentials: username `sosial`, password `sosial`, database `sosial_video`.

## Test users

For local testing only. `web-contact-service` inserts these 5 accounts into `frontend-db` on first start. They are already active and do not need email confirmation.

| Username | Password |
| --- | --- |
| alice | alice123 |
| bob | bob123 |
| carol | carol123 |
| dave | dave123 |
| eve | eve123 |

## Author


|         |                                                     |
| ------- | --------------------------------------------------- |
| First name | Jakob                                               |
| Last name  | Lyse                                                |
| GitHub     | [nordlyse](https://github.com/nordlyse)             |
| Email      | [jakob.lyse@gmail.com](mailto:jakob.lyse@gmail.com) |
