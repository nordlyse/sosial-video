import { useEffect, useRef, useState } from "react";
import { isIOSDevice, streamHasVideo, userEmoji } from "./avatar.js";
import { drawVideoWithWebGpu } from "./webgpu.js";

function prepareVideo(video) {
  video.setAttribute("playsinline", "true");
  video.setAttribute("webkit-playsinline", "true");
  video.playsInline = true;
  video.autoplay = true;
  video.defaultMuted = true;
  video.muted = true;
}

async function playQuietly(node) {
  if (!node) {
    return false;
  }
  try {
    await node.play();
    return true;
  } catch {
    return false;
  }
}

export default function ParticipantTile({ stream, username, role, speaking, compact, local }) {
  const videoRef = useRef(null);
  const audioRef = useRef(null);
  const canvasRef = useRef(null);
  const [gpuReady, setGpuReady] = useState(false);
  const [hasVideo, setHasVideo] = useState(false);
  const [needsTap, setNeedsTap] = useState(false);

  useEffect(() => {
    const update = () => setHasVideo(streamHasVideo(stream));
    update();
    if (!stream) {
      return undefined;
    }
    const tracks = stream.getTracks();
    tracks.forEach((track) => {
      track.addEventListener("ended", update);
      track.addEventListener("mute", update);
      track.addEventListener("unmute", update);
    });
    const timer = setInterval(update, 400);
    return () => {
      clearInterval(timer);
      tracks.forEach((track) => {
        track.removeEventListener("ended", update);
        track.removeEventListener("mute", update);
        track.removeEventListener("unmute", update);
      });
    };
  }, [stream]);

  useEffect(() => {
    const video = videoRef.current;
    const audio = audioRef.current;
    let stop = null;
    let cancelled = false;

    async function attach() {
      if (video) {
        prepareVideo(video);
        video.srcObject = stream || null;
        if (stream) {
          const tryPlay = async () => {
            const played = await playQuietly(video);
            if (!cancelled && played) {
              setNeedsTap(false);
            } else if (!cancelled && !played) {
              setNeedsTap(true);
            }
          };
          video.onloadedmetadata = tryPlay;
          await tryPlay();
        }
      }
      if (audio) {
        audio.muted = Boolean(local);
        audio.srcObject = stream || null;
        if (stream && !local) {
          await playQuietly(audio);
        }
      }
      setGpuReady(false);
      if (!stream || !hasVideo || !video || !canvasRef.current || isIOSDevice() || compact) {
        return;
      }
      const value = await drawVideoWithWebGpu(canvasRef.current, video, () => {
        if (!cancelled) {
          setGpuReady(true);
        }
      });
      if (cancelled) {
        if (typeof value === "function") {
          value();
        }
        return;
      }
      if (typeof value === "function") {
        stop = value;
      }
    }

    attach();
    return () => {
      cancelled = true;
      if (video) {
        video.onloadedmetadata = null;
      }
      if (stop) {
        stop();
      }
    };
  }, [stream, hasVideo, local]);

  async function handleUnlock() {
    const video = videoRef.current;
    const audio = audioRef.current;
    if (video) {
      prepareVideo(video);
      await playQuietly(video);
    }
    if (audio) {
      audio.muted = Boolean(local);
      await playQuietly(audio);
    }
    setNeedsTap(false);
  }

  const emoji = userEmoji(username);
  const showVideo = hasVideo && stream;

  return (
    <div
      className={`tile ${compact ? "compact" : "main"} ${speaking ? "speaking" : ""}`}
      onClick={needsTap || !local ? handleUnlock : undefined}
    >
      {!showVideo ? (
        <div className="tile-avatar">
          <span className="tile-emoji">{emoji}</span>
        </div>
      ) : null}
      <video
        ref={videoRef}
        className={showVideo ? `tile-video ${gpuReady ? "behind" : ""}` : "hidden-video"}
        muted
        playsInline
        autoPlay
      />
      <audio ref={audioRef} autoPlay playsInline />
      {needsTap && showVideo ? (
        <button type="button" className="tile-play glow-button" onClick={handleUnlock}>
          Tap to play
        </button>
      ) : null}
      {showVideo ? (
        <canvas ref={canvasRef} className={`tile-canvas ${gpuReady ? "ready" : ""}`} />
      ) : null}
      <div className="tile-meta">
        <strong>
          {emoji} {username}
        </strong>
        <span>{speaking ? "speaking" : role}</span>
      </div>
    </div>
  );
}
