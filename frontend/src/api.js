function serviceUrl(port, envValue) {
  const fallback = envValue || `http://localhost:${port}`;
  if (typeof window === "undefined") {
    return fallback;
  }
  const { protocol, hostname } = window.location;
  if (!hostname || hostname === "localhost" || hostname === "127.0.0.1") {
    return fallback;
  }
  return `${protocol}//${hostname}:${port}`;
}

export const CONTACT_API = serviceUrl(8081, import.meta.env.VITE_CONTACT_API);
export const WEBRTC_API = serviceUrl(8082, import.meta.env.VITE_WEBRTC_API);

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
      "Could not reach the login server on port 8081. On a phone, use this computer's Wi-Fi IP in the address bar and allow Local Network access."
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
  return request("/v1/login", { method: "POST", body: { username, password } });
}

export function logout(token) {
  return request("/v1/logout", { method: "POST", token });
}

export function fetchContacts(token) {
  return request("/v1/contacts", { token });
}

export function heartbeat(token) {
  return request("/v1/presence", { method: "PUT", token, body: {} });
}

export function fetchStudio(token) {
  return request("/v1/studio", { token });
}

export function startBroadcast(token) {
  return request("/v1/broadcasts", { method: "POST", token });
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
    throw new Error("Could not reach the video server on port 8082 from this device.");
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
    setTimeout(finish, 2500);
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

export async function publishPeer(room, peerId, stream) {
  const pc = new RTCPeerConnection(ICE);
  stream.getTracks().forEach((track) => pc.addTrack(track, stream));
  const offer = await pc.createOffer();
  await pc.setLocalDescription(offer);
  await waitForIce(pc);
  const answer = await webrtcJson(`/v1/rooms/${encodeURIComponent(room)}/publish`, {
    type: pc.localDescription.type,
    sdp: pc.localDescription.sdp,
    peer_id: peerId,
  });
  await pc.setRemoteDescription({ type: answer.type, sdp: answer.sdp });
  return pc;
}

export async function subscribePeer(room, peerId, onStream) {
  const pc = new RTCPeerConnection(ICE);
  pc.addTransceiver("video", { direction: "recvonly" });
  pc.addTransceiver("audio", { direction: "recvonly" });
  pc.ontrack = () => {
    onStream(currentRemoteStream(pc));
  };
  const offer = await pc.createOffer();
  await pc.setLocalDescription(offer);
  await waitForIce(pc);
  const answer = await webrtcJson(
    `/v1/rooms/${encodeURIComponent(room)}/peers/${encodeURIComponent(peerId)}/subscribe`,
    {
      type: pc.localDescription.type,
      sdp: pc.localDescription.sdp,
    }
  );
  await pc.setRemoteDescription({ type: answer.type, sdp: answer.sdp });
  return pc;
}
