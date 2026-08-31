function serviceUrl(port, envValue, proxyPath) {
  const fallback = envValue || `http://localhost:${port}`;
  if (typeof window === "undefined") {
    return fallback;
  }
  const { protocol, hostname, port: pagePort } = window.location;
  if (protocol === "https:" && proxyPath) {
    const portPart = pagePort && pagePort !== "443" ? `:${pagePort}` : "";
    return `${protocol}//${hostname}${portPart}${proxyPath}`;
  }
  if (!hostname || hostname === "localhost" || hostname === "127.0.0.1") {
    return fallback;
  }
  return `${protocol}//${hostname}:${port}`;
}

export const CONTACT_API = serviceUrl(8081, import.meta.env.VITE_CONTACT_API, "/api");
export const WEBRTC_API = serviceUrl(8082, import.meta.env.VITE_WEBRTC_API, "/sfu");

export function browserLocale() {
  if (typeof navigator === "undefined") {
    return "en";
  }
  return (navigator.language || "en").split("-")[0].toLowerCase() || "en";
}

export function httpsAppUrl() {
  if (typeof window === "undefined") {
    return "";
  }
  const { protocol, hostname } = window.location;
  if (protocol === "https:") {
    return "";
  }
  if (!hostname || hostname === "localhost" || hostname === "127.0.0.1") {
    return "";
  }
  return `https://${hostname}:8443`;
}

export function canUseCamera() {
  return Boolean(
    typeof window !== "undefined" &&
      window.isSecureContext &&
      navigator.mediaDevices &&
      typeof navigator.mediaDevices.getUserMedia === "function"
  );
}

export async function openUserMedia() {
  if (!canUseCamera()) {
    const secure = httpsAppUrl();
    throw new Error(
      secure
        ? `Camera and live video need HTTPS. Open ${secure} on this phone, continue past the certificate warning, then allow camera and microphone access.`
        : "Camera is not available in this browser. Allow camera and microphone access."
    );
  }
  try {
    return await navigator.mediaDevices.getUserMedia({
      video: { facingMode: "user" },
      audio: true,
    });
  } catch {
    return navigator.mediaDevices.getUserMedia({ video: true, audio: true });
  }
}

export function unlockMediaPlayback() {
  if (typeof document === "undefined") {
    return;
  }
  document.querySelectorAll("video, audio").forEach((node) => {
    if (node.tagName === "VIDEO") {
      node.muted = true;
      node.defaultMuted = true;
    }
    node.play().catch(() => {});
  });
}

function signalingHost() {
  if (typeof window === "undefined") {
    return "";
  }
  return window.location.hostname;
}

const SESSION_KEY = "sosial-video-session";
const ICE = { iceServers: [{ urls: "stun:stun.l.google.com:19302" }] };

export function loadSession() {
  try {
    const raw = sessionStorage.getItem(SESSION_KEY);
    return raw ? JSON.parse(raw) : null;
  } catch {
    return null;
  }
}

export function saveSession(session) {
  sessionStorage.setItem(SESSION_KEY, JSON.stringify(session));
}

export function clearSession() {
  sessionStorage.removeItem(SESSION_KEY);
}

async function request(path, { method = "GET", token, body } = {}) {
  const headers = {};
  if (body !== undefined) {
    headers["Content-Type"] = "application/json";
  }
  if (token) {
    headers.Authorization = `Bearer ${token}`;
  }
  let response;
  try {
    response = await fetch(`${CONTACT_API}${path}`, {
      method,
      headers,
      body: body !== undefined ? JSON.stringify(body) : undefined,
    });
  } catch {
    throw new Error(
      "Could not reach the login server. On a phone, open this computer's Wi-Fi IP with HTTPS on port 8443 and allow Local Network access."
    );
  }
  if (response.status === 204) {
    return null;
  }
  const data = await response.json().catch(() => ({}));
  if (!response.ok) {
    throw new Error(data.error || `Request failed (${response.status})`);
  }
  return data;
}

export function login(username, password) {
  return request("/v1/login", {
    method: "POST",
    body: { username, password, locale: browserLocale() },
  });
}

export function register(username, email, password) {
  return request("/v1/register", {
    method: "POST",
    body: { username, email, password, locale: browserLocale() },
  });
}

export function confirmAccount({ token, code } = {}) {
  return request("/v1/verify", { method: "POST", body: { token, code } });
}

export function logout(token) {
  return request("/v1/logout", { method: "POST", token });
}

export function fetchContacts(token) {
  return request("/v1/contacts", { token });
}

export function heartbeat(token) {
  return request("/v1/presence", { method: "PUT", token, body: { locale: browserLocale() } });
}

export function fetchStudio(token) {
  return request("/v1/studio", { token });
}

export function fetchPublicBroadcasts(token, q = "") {
  const query = q.trim() ? `?q=${encodeURIComponent(q.trim())}` : "";
  return request(`/v1/public-broadcasts${query}`, { token });
}

export function startBroadcast(token, title, isPublic = true) {
  return request("/v1/broadcasts", {
    method: "POST",
    token,
    body: { title, is_public: isPublic },
  });
}

export function leaveBroadcast(token) {
  return request("/v1/broadcasts/current/leave", { method: "POST", token });
}

export function endBroadcast(token) {
  return request("/v1/broadcasts/current/end", { method: "POST", token });
}

export function requestJoin(token, broadcastId) {
  return request(`/v1/broadcasts/${broadcastId}/requests`, { method: "POST", token });
}

export function acceptJoin(token, broadcastId, requestId, role) {
  return request(`/v1/broadcasts/${broadcastId}/requests/${requestId}/accept`, {
    method: "POST",
    token,
    body: { role },
  });
}

export function rejectJoin(token, broadcastId, requestId) {
  return request(`/v1/broadcasts/${broadcastId}/requests/${requestId}/reject`, {
    method: "POST",
    token,
  });
}

export function setSpeaking(token, broadcastId, speaking) {
  return request(`/v1/broadcasts/${broadcastId}/speaking`, {
    method: "PUT",
    token,
    body: { speaking },
  });
}

export function fetchComments(token, userId) {
  return request(`/v1/users/${userId}/comments`, { token });
}

export function postComment(token, userId, body, extra = {}) {
  return request(`/v1/users/${userId}/comments`, {
    method: "POST",
    token,
    body: {
      body,
      parent_id: extra.parentId ?? null,
      is_private: Boolean(extra.isPrivate),
    },
  });
}

export function sendReaction(token, broadcastId, emoji) {
  return request(`/v1/broadcasts/${broadcastId}/reactions`, {
    method: "POST",
    token,
    body: { emoji },
  });
}

async function webrtcJson(path, body) {
  let response;
  try {
    response = await fetch(`${WEBRTC_API}${path}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
  } catch {
    throw new Error("Could not reach the video server from this device.");
  }
  const data = await response.json().catch(() => ({}));
  if (!response.ok) {
    throw new Error(data.error || "Could not establish a WebRTC connection");
  }
  return data;
}

function waitForIce(pc) {
  if (pc.iceGatheringState === "complete") {
    return Promise.resolve();
  }
  return new Promise((resolve) => {
    const finish = () => {
      pc.removeEventListener("icegatheringstatechange", onChange);
      resolve();
    };
    const onChange = () => {
      if (pc.iceGatheringState === "complete") {
        finish();
      }
    };
    pc.addEventListener("icegatheringstatechange", onChange);
    setTimeout(finish, 4000);
  });
}

function currentRemoteStream(pc) {
  const tracks = pc
    .getReceivers()
    .map((receiver) => receiver.track)
    .filter((track) => track && track.readyState !== "ended");
  return new MediaStream(tracks);
}

export async function listPublishers(room) {
  const response = await fetch(`${WEBRTC_API}/v1/rooms/${encodeURIComponent(room)}`);
  if (!response.ok) {
    throw new Error("Could not read room status");
  }
  return response.json();
}

async function primeLocalStream(stream) {
  const videoTrack = stream.getVideoTracks()[0];
  if (!videoTrack) {
    return () => {};
  }
  videoTrack.enabled = true;
  const pump = document.createElement("video");
  pump.setAttribute("playsinline", "true");
  pump.setAttribute("webkit-playsinline", "true");
  pump.muted = true;
  pump.defaultMuted = true;
  pump.playsInline = true;
  pump.autoplay = true;
  pump.srcObject = stream;
  await pump.play().catch(() => {});
  if (videoTrack.muted) {
    await Promise.race([
      new Promise((resolve) => videoTrack.addEventListener("unmute", resolve, { once: true })),
      new Promise((resolve) => setTimeout(resolve, 1500)),
    ]);
  }
  return () => {
    pump.srcObject = null;
    pump.remove();
  };
}

export async function publishPeer(room, peerId, stream) {
  const pc = new RTCPeerConnection(ICE);
  const stopPrime = await primeLocalStream(stream);
  const sendStream = new MediaStream();
  for (const track of stream.getTracks()) {
    if (track.readyState === "ended") {
      continue;
    }
    track.enabled = true;
    const outbound = track.clone();
    outbound.enabled = true;
    sendStream.addTrack(outbound);
    pc.addTrack(outbound, sendStream);
  }
  if (sendStream.getTracks().length === 0) {
    stopPrime();
    pc.close();
    throw new Error("Camera is on, but no video or audio track is available to send.");
  }
  const stopClones = () => {
    sendStream.getTracks().forEach((track) => track.stop());
    stopPrime();
  };
  pc.addEventListener("connectionstatechange", () => {
    if (pc.connectionState === "failed" || pc.connectionState === "closed") {
      stopClones();
    }
  });
  const close = pc.close.bind(pc);
  pc.close = () => {
    stopClones();
    close();
  };
  const offer = await pc.createOffer();
  await pc.setLocalDescription(offer);
  await waitForIce(pc);
  const answer = await webrtcJson(`/v1/rooms/${encodeURIComponent(room)}/publish`, {
    type: pc.localDescription.type,
    sdp: pc.localDescription.sdp,
    peer_id: peerId,
    client_host: signalingHost(),
  });
  await pc.setRemoteDescription({ type: answer.type, sdp: answer.sdp });
  return pc;
}

export async function subscribePeer(room, peerId, onStream) {
  const pc = new RTCPeerConnection(ICE);
  pc.addTransceiver("video", { direction: "recvonly" });
  pc.addTransceiver("audio", { direction: "recvonly" });
  const emit = () => {
    const fromEvent = currentRemoteStream(pc);
    if (fromEvent.getTracks().length > 0) {
      onStream(fromEvent);
    }
  };
  pc.ontrack = (event) => {
    const inbound = event.streams[0] || currentRemoteStream(pc);
    if (event.track) {
      event.track.onunmute = emit;
      event.track.onmute = emit;
    }
    onStream(inbound);
  };
  pc.addEventListener("connectionstatechange", () => {
    if (pc.connectionState === "connected") {
      emit();
    }
  });
  const offer = await pc.createOffer();
  await pc.setLocalDescription(offer);
  await waitForIce(pc);
  const answer = await webrtcJson(
    `/v1/rooms/${encodeURIComponent(room)}/peers/${encodeURIComponent(peerId)}/subscribe`,
    {
      type: pc.localDescription.type,
      sdp: pc.localDescription.sdp,
      client_host: signalingHost(),
    }
  );
  await pc.setRemoteDescription({ type: answer.type, sdp: answer.sdp });
  emit();
  return pc;
}
