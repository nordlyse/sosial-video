import { useEffect, useRef, useState } from "react";
import { streamHasVideo, userEmoji } from "./avatar.js";
import { drawVideoWithWebGpu } from "./webgpu.js";

export default function ParticipantTile({ stream, username, role, speaking, compact, local }) {
  const videoRef = useRef(null);
  const audioRef = useRef(null);
  const canvasRef = useRef(null);
  const [gpuReady, setGpuReady] = useState(false);
  const [hasVideo, setHasVideo] = useState(false);

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
    const timer = setInterval(update, 500);
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
    if (video) {
      video.srcObject = stream || null;
      if (stream) {
        video.play().catch(() => {});
      }
    }
    if (audio) {
      audio.muted = Boolean(local);
      audio.srcObject = stream || null;
      if (stream && !local) {
        audio.play().catch(() => {});
      }
    }
    let stop = null;
    let cancelled = false;
    setGpuReady(false);
    if (stream && hasVideo && video && canvasRef.current) {
      drawVideoWithWebGpu(canvasRef.current, video, () => {
        if (!cancelled) {
          setGpuReady(true);
        }
      }).then((value) => {
        if (cancelled) {
          if (typeof value === "function") {
            value();
          }
          return;
        }
        if (typeof value === "function") {
          stop = value;
        }
      });
    }
    return () => {
      cancelled = true;
      if (stop) {
        stop();
      }
    };
  }, [stream, hasVideo, local]);

  const emoji = userEmoji(username);

  return (
    <div className={`tile ${compact ? "compact" : "main"} ${speaking ? "speaking" : ""}`}>
      {!hasVideo ? (
        <div className="tile-avatar">
          <span className="tile-emoji">{emoji}</span>
        </div>
      ) : null}
      <video
        ref={videoRef}
        className={hasVideo ? `tile-video ${gpuReady ? "behind" : ""}` : "hidden-video"}
        muted
        playsInline
        autoPlay
      />
      <audio ref={audioRef} autoPlay />
      {hasVideo ? (
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
