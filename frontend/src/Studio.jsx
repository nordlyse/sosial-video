import { useEffect, useRef, useState } from "react";
import {
  acceptJoin,
  canUseCamera,
  endBroadcast,
  fetchContacts,
  fetchPublicBroadcasts,
  fetchStudio,
  heartbeat,
  httpsAppUrl,
  leaveBroadcast,
  listPublishers,
  logout,
  openUserMedia,
  publishPeer,
  rejectJoin,
  requestJoin,
  sendReaction,
  setSpeaking,
  startBroadcast,
  subscribePeer,
  unlockMediaPlayback,
} from "./api.js";
import { userEmoji } from "./avatar.js";
import GradientText from "./bits/GradientText.jsx";
import ParticlesField from "./bits/ParticlesField.jsx";
import SpotlightCard from "./bits/SpotlightCard.jsx";
import CommentsPanel from "./CommentsPanel.jsx";
import ParticipantTile from "./ParticipantTile.jsx";
import ReactionOverlay, { ReactionBar } from "./ReactionOverlay.jsx";

export default function Studio({ session, onLogout }) {
  const [contacts, setContacts] = useState([]);
  const [studio, setStudio] = useState({
    broadcasts: [],
    public_broadcasts: [],
    membership: null,
    incoming_requests: [],
    outgoing_requests: [],
    participants: [],
    recent_reactions: [],
  });
  const [status, setStatus] = useState("");
  const [localStream, setLocalStream] = useState(null);
  const [remoteStreams, setRemoteStreams] = useState({});
  const [lobbyTab, setLobbyTab] = useState("camera");
  const [cameraUser, setCameraUser] = useState(() => session.user);
  const [titleDraft, setTitleDraft] = useState("");
  const [publicQuery, setPublicQuery] = useState("");
  const [publicResults, setPublicResults] = useState(null);
  const [showStartForm, setShowStartForm] = useState(false);
  const httpsHint = httpsAppUrl();
  const localStreamRef = useRef(null);
  const publishPcRef = useRef(null);
  const publishKeyRef = useRef("");
  const subsRef = useRef(new Map());
  const speakingRef = useRef(false);
  const analyserRef = useRef(null);
  const lastReactAtRef = useRef(0);
  const [reactionBurst, setReactionBurst] = useState(null);
  const syncLockRef = useRef(false);

  const membership = studio.membership;
  const me = session.user.username;
  const roomId = membership?.room_id;
  const membershipRole = membership?.role;
  const broadcastId = membership?.broadcast_id;

  useEffect(() => {
    localStreamRef.current = localStream;
  }, [localStream]);

  useEffect(() => {
    if (!broadcastId || localStreamRef.current) {
      return undefined;
    }
    if (!canUseCamera()) {
      if (httpsHint) {
        setStatus(
          `Camera and live video need HTTPS. Open ${httpsHint} on this phone, continue past the certificate warning, then allow camera access.`
        );
      }
      return undefined;
    }
    openCamera().catch(() => {});
    return undefined;
  }, [broadcastId, httpsHint]);

  useEffect(() => {
    let cancelled = false;
    async function refresh() {
      try {
        await heartbeat(session.token);
        const contactList = await fetchContacts(session.token);
        if (!cancelled) {
          setContacts(contactList);
        }
        try {
          const snapshot = await fetchStudio(session.token);
          if (!cancelled) {
            setStudio({ recent_reactions: [], ...snapshot });
          }
        } catch (err) {
          if (!cancelled) {
            setStatus(`Live broadcasts unavailable: ${err.message}`);
          }
        }
      } catch (err) {
        if (!cancelled) {
          setStatus(err.message);
        }
      }
    }
    refresh();
    const timer = setInterval(refresh, 1000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [session.token]);

  useEffect(() => {
    if (!roomId) {
      closeRoom();
      return undefined;
    }
    let cancelled = false;
    async function syncRoom() {
      if (syncLockRef.current) {
        return;
      }
      syncLockRef.current = true;
      try {
        const info = await listPublishers(roomId);
        if (cancelled) {
          return;
        }
        const wanted = new Set();
        for (const publisher of info.publishers || []) {
          if (publisher.peer_id === me) {
            continue;
          }
          if (!publisher.has_video && !publisher.has_audio) {
            continue;
          }
          const key = `${publisher.peer_id}:${publisher.has_video}:${publisher.has_audio}`;
          wanted.add(publisher.peer_id);
          const current = subsRef.current.get(publisher.peer_id);
          if (current?.key === key) {
            continue;
          }
          if (current) {
            current.pc.close();
          }
          const pc = await subscribePeer(roomId, publisher.peer_id, (stream) => {
            setRemoteStreams((prev) => ({ ...prev, [publisher.peer_id]: stream }));
          });
          if (cancelled) {
            pc.close();
            return;
          }
          subsRef.current.set(publisher.peer_id, { pc, key });
        }
        for (const [peerId, sub] of subsRef.current) {
          if (!wanted.has(peerId)) {
            sub.pc.close();
            subsRef.current.delete(peerId);
            setRemoteStreams((prev) => {
              const next = { ...prev };
              delete next[peerId];
              return next;
            });
          }
        }
      } catch (err) {
        if (!cancelled) {
          setStatus(err.message);
        }
      } finally {
        syncLockRef.current = false;
      }
    }
    syncRoom();
    const timer = setInterval(syncRoom, 1000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [roomId, me]);

  useEffect(() => {
    if (!roomId || !localStream) {
      return undefined;
    }
    const tracks = localStream.getTracks().filter((track) => track.readyState !== "ended");
    if (tracks.length === 0) {
      return undefined;
    }
    tracks.forEach((track) => {
      track.enabled = true;
    });
    const key = `${roomId}:${tracks.map((track) => `${track.kind}:${track.id}`).join(",")}`;
    if (publishKeyRef.current === key && publishPcRef.current) {
      return undefined;
    }
    let cancelled = false;
    async function publish() {
      try {
        if (publishPcRef.current) {
          publishPcRef.current.close();
          publishPcRef.current = null;
        }
        const outbound = new MediaStream(tracks);
        const pc = await publishPeer(roomId, me, outbound);
        if (cancelled) {
          pc.close();
          return;
        }
        publishPcRef.current = pc;
        publishKeyRef.current = key;
        setStatus("Camera is sending to the room.");
      } catch (err) {
        publishKeyRef.current = "";
        if (!cancelled) {
          setStatus(err.message);
        }
      }
    }
    publish();
    return () => {
      cancelled = true;
    };
  }, [roomId, localStream, me]);

  useEffect(() => {
    if (!broadcastId || !localStream || membershipRole === "listener") {
      return undefined;
    }
    const audio = localStream.getAudioTracks()[0];
    if (!audio) {
      return undefined;
    }
    const context = new AudioContext();
    const source = context.createMediaStreamSource(new MediaStream([audio]));
    const analyser = context.createAnalyser();
    analyser.fftSize = 512;
    source.connect(analyser);
    analyserRef.current = analyser;
    const data = new Uint8Array(analyser.frequencyBinCount);
    const timer = setInterval(() => {
      analyser.getByteFrequencyData(data);
      const avg = data.reduce((sum, value) => sum + value, 0) / data.length;
      const speaking = avg > 18;
      if (speaking === speakingRef.current) {
        return;
      }
      speakingRef.current = speaking;
      setSpeaking(session.token, broadcastId, speaking).catch(() => {});
    }, 250);
    return () => {
      clearInterval(timer);
      context.close().catch(() => {});
      if (speakingRef.current) {
        speakingRef.current = false;
        setSpeaking(session.token, broadcastId, false).catch(() => {});
      }
    };
  }, [broadcastId, membershipRole, localStream, session.token]);

  useEffect(() => {
    if (lobbyTab !== "public") {
      return undefined;
    }
    let cancelled = false;
    async function refreshPublic() {
      try {
        const list = await fetchPublicBroadcasts(session.token, publicQuery);
        if (!cancelled) {
          setPublicResults(list);
        }
      } catch (err) {
        if (!cancelled) {
          setStatus(err.message);
        }
      }
    }
    refreshPublic();
    const timer = setInterval(refreshPublic, 2000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [lobbyTab, publicQuery, session.token]);

  useEffect(() => {
    return () => {
      closeRoom();
      stopCamera();
    };
  }, []);

  function closeRoom() {
    if (publishPcRef.current) {
      publishPcRef.current.close();
      publishPcRef.current = null;
    }
    publishKeyRef.current = "";
    for (const sub of subsRef.current.values()) {
      sub.pc.close();
    }
    subsRef.current.clear();
    setRemoteStreams({});
  }

  function stopCamera() {
    if (localStreamRef.current) {
      localStreamRef.current.getTracks().forEach((track) => track.stop());
      localStreamRef.current = null;
      setLocalStream(null);
    }
  }

  async function reloadStudio() {
    const snapshot = await fetchStudio(session.token);
    setStudio({ recent_reactions: [], ...snapshot });
    return snapshot;
  }

  async function handleLeave() {
    try {
      await leaveBroadcast(session.token);
      closeRoom();
      await reloadStudio();
      setStatus("You left the broadcast.");
    } catch (err) {
      setStatus(err.message);
    }
  }

  async function handleLogout() {
    try {
      await leaveBroadcast(session.token);
    } catch {
      // Continue signing out even if leave fails.
    }
    try {
      await logout(session.token);
    } catch {
      // Local logout still proceeds.
    }
    closeRoom();
    stopCamera();
    onLogout();
  }

  async function handleSendCamera() {
    unlockMediaPlayback();
    try {
      if (!localStreamRef.current) {
        await openCamera();
        return;
      }
      publishKeyRef.current = "";
      setLocalStream(new MediaStream(localStreamRef.current.getTracks()));
      setStatus("Sending camera to the room...");
    } catch (err) {
      setStatus(err.message);
    }
  }

  async function openCamera() {
    setStatus("");
    try {
      const stream = await openUserMedia();
      localStreamRef.current = stream;
      setLocalStream(stream);
      unlockMediaPlayback();
      setStatus("Camera is on. Start a broadcast or ask to join one.");
      setLobbyTab("camera");
      setCameraUser(session.user);
      return stream;
    } catch (err) {
      setStatus(err.message || "Could not open the camera. Allow camera and microphone access.");
      throw err;
    }
  }

  function handleStartClick() {
    setShowStartForm(true);
    setTitleDraft((prev) => prev || "#");
  }

  async function handleStartBroadcast(event) {
    event.preventDefault();
    try {
      setStatus("Starting broadcast...");
      if (!localStreamRef.current && canUseCamera()) {
        await openCamera();
      }
      const created = await startBroadcast(session.token, titleDraft, true);
      setShowStartForm(false);
      setStudio((prev) => ({ ...prev, membership: created }));
      await reloadStudio();
      setStatus("Broadcast is live. Join requests will appear at the top of your video.");
    } catch (err) {
      setStatus(err.message);
    }
  }

  async function handleEndBroadcast() {
    try {
      await endBroadcast(session.token);
      closeRoom();
      await reloadStudio();
      setStatus("Broadcast ended.");
    } catch (err) {
      setStatus(err.message);
    }
  }

  async function handleAskToJoin(id) {
    try {
      if (!localStreamRef.current && canUseCamera()) {
        await openCamera();
      }
      unlockMediaPlayback();
      await requestJoin(session.token, id);
      await reloadStudio();
      setStatus("Join request sent. The host will see it on their screen.");
    } catch (err) {
      setStatus(err.message);
    }
  }

  async function handleAccept(request, role) {
    try {
      unlockMediaPlayback();
      await acceptJoin(session.token, request.broadcast_id, request.id, role);
      await reloadStudio();
      setStatus(`${request.from.username} joined as ${role}.`);
    } catch (err) {
      setStatus(err.message);
    }
  }

  async function handleReject(request) {
    try {
      await rejectJoin(session.token, request.broadcast_id, request.id);
      await reloadStudio();
    } catch (err) {
      setStatus(err.message);
    }
  }

  async function handleReact(emoji) {
    const now = Date.now();
    if (now - lastReactAtRef.current < 280) {
      return;
    }
    lastReactAtRef.current = now;
    setReactionBurst({ emoji, nonce: now });
    if (!broadcastId) {
      return;
    }
    try {
      const created = await sendReaction(session.token, broadcastId, emoji);
      setStudio((prev) => ({
        ...prev,
        recent_reactions: [...(prev.recent_reactions || []), created],
      }));
    } catch (err) {
      setStatus(err.message);
    }
  }

  function openCameraPage(user) {
    setCameraUser(user);
    setLobbyTab("camera");
  }

  const speaker =
    studio.participants.find((participant) => participant.speaking) ||
    studio.participants.find((participant) => participant.role === "host") ||
    studio.participants[0];
  const others = studio.participants.filter(
    (participant) => !speaker || participant.user.username !== speaker.user.username
  );
  const inCall = Boolean(membership);
  const hostView = membership?.role === "host";
  const incoming = studio.incoming_requests || [];
  const viewingSelf = cameraUser?.username === me;

  function streamFor(username) {
    if (username === me) {
      return localStream;
    }
    return remoteStreams[username];
  }

  function liveBroadcastFor(username) {
    return studio.broadcasts.find((broadcast) => broadcast.host.username === username);
  }

  const mainStream = speaker
    ? streamFor(speaker.user.username) || (speaker.user.username === me ? localStream : null)
    : localStream;
  const mainName = speaker?.user.username || me;
  const mainRole = speaker?.role || (inCall ? membershipRole : "you");
  const showMain = Boolean(speaker || localStream);
  const railPeople = inCall ? others : [];
  const publicList = publicResults ?? studio.public_broadcasts ?? [];

  function broadcastRow(broadcast) {
    const outgoing = studio.outgoing_requests.find((item) => item.broadcast_id === broadcast.id);
    const isHost = broadcast.host.username === me;
    const tags = broadcast.tags || [];
    return (
      <li key={broadcast.id}>
        <div className="broadcast-copy">
          <strong>{broadcast.title || `${broadcast.host.username}'s broadcast`}</strong>
          <span className="dot on" />
          <em>
            {broadcast.host.username} · {broadcast.member_count} in room
            {outgoing ? ` · ${outgoing.status}` : ""}
          </em>
          {tags.length > 0 ? (
            <p className="tag-row">
              {tags.map((tag) => (
                <button
                  key={tag}
                  type="button"
                  className="tag-chip"
                  onClick={() => {
                    setPublicQuery(`#${tag}`);
                    setLobbyTab("public");
                  }}
                >
                  #{tag}
                </button>
              ))}
            </p>
          ) : null}
        </div>
        <div className="row-actions">
          <button type="button" className="tiny ghost" onClick={() => openCameraPage(broadcast.host)}>
            Open camera
          </button>
          {!isHost ? (
            <button
              type="button"
              className="tiny glow-button"
              disabled={outgoing?.status === "pending"}
              onClick={() => handleAskToJoin(broadcast.id)}
            >
              {outgoing?.status === "pending" ? "Waiting for host" : "Ask to join"}
            </button>
          ) : null}
        </div>
      </li>
    );
  }

  return (
    <main className={`studio ${inCall ? "in-call" : "lobby"}`}>
      {!inCall ? <ParticlesField /> : null}
      <header>
        <div>
          <p className="eyebrow">Sosial Video</p>
          <h1>
            {inCall ? (
              <GradientText>
                {membership.title
                  ? membership.title
                  : `${membership.host.username}'s broadcast`}
              </GradientText>
            ) : (
              <GradientText>{me}</GradientText>
            )}
          </h1>
        </div>
        <div className="header-actions">
          <button type="button" className="glow-button" onClick={inCall ? handleSendCamera : openCamera}>
            {inCall ? (localStream ? "Resend camera" : "Send camera") : localStream ? "Camera on" : "Open camera"}
          </button>
          {hostView ? (
            <button type="button" className="ghost" onClick={handleEndBroadcast}>
              End broadcast
            </button>
          ) : membership ? (
            <button type="button" className="ghost" onClick={handleLeave}>
              Leave broadcast
            </button>
          ) : (
            <button type="button" className="glow-button" onClick={handleStartClick}>
              Start broadcast
            </button>
          )}
          <button type="button" className="ghost" onClick={handleLogout}>
            Sign out
          </button>
        </div>
        {httpsHint && !inCall ? (
          <p className="secure-banner">
            On a phone, open <a href={httpsHint}>{httpsHint}</a> so camera and live video can work, then
            continue past the certificate warning.
          </p>
        ) : null}
      </header>

      {showStartForm && !inCall ? (
        <form className="start-form" onSubmit={handleStartBroadcast}>
          <h2>Start a public broadcast</h2>
          <p className="hint">
            Add a title with topics as hashtags, for example <code>#discussion #politics</code>. Other
            people can search those tags even if they are not contacts.
          </p>
          <label>
            Title
            <input
              value={titleDraft}
              onChange={(event) => setTitleDraft(event.target.value)}
              placeholder="#discussion #politics"
              maxLength={120}
              autoFocus
            />
          </label>
          <div className="request-actions">
            <button type="submit" className="glow-button">
              Go live
            </button>
            <button type="button" className="ghost" onClick={() => setShowStartForm(false)}>
              Cancel
            </button>
          </div>
        </form>
      ) : null}

      {inCall ? (
        <section className={`call ${railPeople.length > 0 ? "with-rail" : "solo"}`}>
          <div className="main-stage">
            {hostView && incoming.length > 0 ? (
              <div className="join-banner">
                {incoming.map((item) => (
                  <div key={item.id} className="join-banner-card">
                    <p>
                      <strong>{item.from.username}</strong> wants to join your broadcast
                    </p>
                    <div className="request-actions">
                      <button type="button" className="glow-button" onClick={() => handleAccept(item, "listener")}>
                        Accept as listener
                      </button>
                      <button type="button" className="glow-button" onClick={() => handleAccept(item, "speaker")}>
                        Accept as speaker
                      </button>
                      <button type="button" className="ghost" onClick={() => handleReject(item)}>
                        Decline
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            ) : null}
            {showMain ? (
              <ParticipantTile
                stream={mainStream}
                username={mainName}
                role={mainRole}
                speaking={Boolean(speaker?.speaking)}
                local={mainName === me}
              />
            ) : (
              <div className="empty-stage">
                {inCall
                  ? "Waiting for live video. On a phone, open this app over HTTPS on port 8443."
                  : "Open your camera, then start a broadcast."}
              </div>
            )}
            <ReactionOverlay
              key={broadcastId}
              reactions={studio.recent_reactions || []}
              burst={reactionBurst}
              selfUsername={me}
            />
            <ReactionBar onReact={handleReact} />
            <aside className="call-comments">
              <CommentsPanel
                token={session.token}
                broadcastId={broadcastId}
                targetUser={membership.host}
                currentUser={session.user}
                compact
              />
            </aside>
            {status ? <p className="call-status">{status}</p> : null}
          </div>

          {railPeople.length > 0 ? (
            <aside className="rail">
              {railPeople.map((participant) => (
                <ParticipantTile
                  key={participant.user.id}
                  stream={streamFor(participant.user.username)}
                  username={participant.user.username}
                  role={participant.role}
                  speaking={participant.speaking}
                  compact
                  local={participant.user.username === me}
                />
              ))}
            </aside>
          ) : null}
        </section>
      ) : null}

      {!inCall ? (
        <>
          <nav className="lobby-tabs" aria-label="Studio sections">
            <button
              type="button"
              className={lobbyTab === "public" ? "active" : ""}
              onClick={() => setLobbyTab("public")}
            >
              Public
            </button>
            <button
              type="button"
              className={lobbyTab === "live" ? "active" : ""}
              onClick={() => setLobbyTab("live")}
            >
              Live broadcasts
            </button>
            <button
              type="button"
              className={lobbyTab === "camera" ? "active" : ""}
              onClick={() => setLobbyTab("camera")}
            >
              Camera
            </button>
            <button
              type="button"
              className={lobbyTab === "contacts" ? "active" : ""}
              onClick={() => setLobbyTab("contacts")}
            >
              Contacts
            </button>
          </nav>
          <section className={`lobby-panel tab-${lobbyTab}`}>
            {lobbyTab === "public" ? (
              <SpotlightCard>
                <h2>Public broadcasts</h2>
                <p className="hint">
                  Search by hashtag such as #discussion. Without a search, the 10 busiest live rooms in
                  your language are shown.
                </p>
                <label className="search-label">
                  Search
                  <input
                    value={publicQuery}
                    onChange={(event) => setPublicQuery(event.target.value)}
                    placeholder="#politics"
                  />
                </label>
                <ul>
                  {publicList.length === 0 ? (
                    <li>No public live broadcasts match this search yet.</li>
                  ) : null}
                  {publicList.map((broadcast) => broadcastRow(broadcast))}
                </ul>
              </SpotlightCard>
            ) : null}

            {lobbyTab === "live" ? (
              <SpotlightCard>
                <h2>Live broadcasts</h2>
                <p className="hint">Ask to join. The host sees your request on their video and can accept it.</p>
                <ul>
                  {studio.broadcasts.length === 0 ? <li>No live broadcasts yet.</li> : null}
                  {studio.broadcasts.map((broadcast) => broadcastRow(broadcast))}
                </ul>
              </SpotlightCard>
            ) : null}

            {lobbyTab === "camera" ? (
              <div className="camera-tab">
                <SpotlightCard className="preview-card">
                  <div className="camera-heading">
                    <div>
                      <h2>{viewingSelf ? "Camera" : `${cameraUser.username}'s camera`}</h2>
                      <p className="hint">
                        {viewingSelf
                          ? "Preview yourself here, then start a broadcast."
                          : "Comments on this camera appear on the right. Ask to join if they are live."}
                      </p>
                    </div>
                    {!viewingSelf ? (
                      <button type="button" className="tiny ghost" onClick={() => openCameraPage(session.user)}>
                        My camera
                      </button>
                    ) : null}
                  </div>
                  {viewingSelf && localStream ? (
                    <ParticipantTile
                      stream={localStream}
                      username={me}
                      role="you"
                      speaking={false}
                      compact
                      local
                    />
                  ) : viewingSelf ? (
                    <div className="empty-stage">Open your camera, then start a broadcast.</div>
                  ) : (
                    <div className="empty-stage camera-other">
                      <span className="tile-emoji">{userEmoji(cameraUser.username)}</span>
                      <p>
                        {liveBroadcastFor(cameraUser.username)
                          ? `${cameraUser.username} is live now.`
                          : `${cameraUser.username} is not broadcasting.`}
                      </p>
                    </div>
                  )}
                  {status ? <p className="hint">{status}</p> : null}
                </SpotlightCard>
                <div className="comments-card">
                  <CommentsPanel
                    token={session.token}
                    broadcastId={liveBroadcastFor(cameraUser.username)?.id}
                    targetUser={cameraUser}
                    currentUser={session.user}
                  />
                </div>
              </div>
            ) : null}

            {lobbyTab === "contacts" ? (
              <SpotlightCard>
                <h2>Contacts</h2>
                <p className="hint">Open someone’s camera to comment, or ask to join if they are live.</p>
                <ul>
                  {contacts.map((contact) => {
                    const live = liveBroadcastFor(contact.username);
                    const outgoing = live
                      ? studio.outgoing_requests.find((item) => item.broadcast_id === live.id)
                      : null;
                    const isMe = contact.username === me;
                    return (
                      <li key={contact.id}>
                        <strong>{contact.username}</strong>
                        <span className={contact.online || live ? "dot on" : "dot"} />
                        <em>{live ? "live now" : contact.online ? "online" : "offline"}</em>
                        <div className="row-actions">
                          <button
                            type="button"
                            className="tiny ghost"
                            onClick={() => openCameraPage(contact)}
                          >
                            Open camera
                          </button>
                          {live && !isMe ? (
                            <button
                              type="button"
                              className="tiny glow-button"
                              disabled={outgoing?.status === "pending"}
                              onClick={() => handleAskToJoin(live.id)}
                            >
                              {outgoing?.status === "pending" ? "Waiting for host" : "Ask to join"}
                            </button>
                          ) : null}
                        </div>
                      </li>
                    );
                  })}
                </ul>
              </SpotlightCard>
            ) : null}
          </section>
        </>
      ) : null}
    </main>
  );
}
