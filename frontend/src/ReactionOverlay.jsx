import { useEffect, useRef, useState } from "react";

const REACTIONS = ["❤️", "👍", "👎", "😂", "🔥", "👏", "😮", "😢", "🎉"];

export function ReactionBar({ onReact, disabled }) {
  const barRef = useRef(null);
  const onReactRef = useRef(onReact);
  onReactRef.current = onReact;

  useEffect(() => {
    const node = barRef.current;
    if (!node) {
      return undefined;
    }
    function handleClick(event) {
      const button = event.target.closest("button.reaction-button");
      if (!button || disabled) {
        return;
      }
      const emoji = (button.textContent || "").trim();
      if (emoji) {
        onReactRef.current(emoji);
      }
    }
    node.addEventListener("click", handleClick);
    return () => node.removeEventListener("click", handleClick);
  }, [disabled]);

  return (
    <div className="reaction-bar" ref={barRef}>
      {REACTIONS.map((emoji) => (
        <button
          key={emoji}
          type="button"
          className="reaction-button"
          disabled={disabled}
          aria-label={`Send ${emoji}`}
        >
          {emoji}
        </button>
      ))}
    </div>
  );
}

export default function ReactionOverlay({ reactions, burst, selfUsername }) {
  const seenRef = useRef(new Set());
  const primedRef = useRef(false);
  const lastBurstRef = useRef(null);
  const [floaters, setFloaters] = useState([]);

  function spawnBurst(emoji, reactionId) {
    const spawned = burstEmojis(emoji, reactionId);
    setFloaters((prev) => [...prev, ...spawned]);
  }

  useEffect(() => {
    if (!burst || burst.nonce === lastBurstRef.current) {
      return;
    }
    lastBurstRef.current = burst.nonce;
    spawnBurst(burst.emoji, `local-${burst.nonce}`);
  }, [burst]);

  useEffect(() => {
    const incoming = reactions || [];
    if (!primedRef.current) {
      incoming.forEach((item) => seenRef.current.add(item.id));
      primedRef.current = true;
      return;
    }
    for (const item of incoming) {
      if (seenRef.current.has(item.id)) {
        continue;
      }
      seenRef.current.add(item.id);
      if (item.from?.username === selfUsername) {
        continue;
      }
      spawnBurst(item.emoji, item.id);
    }
  }, [reactions, selfUsername]);

  function removeFloater(key) {
    setFloaters((prev) => prev.filter((item) => item.key !== key));
  }

  return (
    <div className="reaction-overlay" aria-hidden="true">
      {floaters.map((item) => (
        <span
          key={item.key}
          className="reaction-floater"
          style={{
            left: `${item.left}%`,
            animationDuration: `${item.duration}ms`,
            animationDelay: `${item.delay}ms`,
            fontSize: `${item.size}rem`,
            "--drift": `${item.drift}px`,
          }}
          onAnimationEnd={() => removeFloater(item.key)}
        >
          {item.emoji}
        </span>
      ))}
    </div>
  );
}

function burstEmojis(emoji, reactionId) {
  const count = 14 + Math.floor(Math.random() * 8);
  return Array.from({ length: count }, (_, index) => ({
    key: `${reactionId}-${index}-${Math.random().toString(36).slice(2, 7)}`,
    emoji,
    left: 8 + Math.random() * 84,
    duration: 2600 + Math.random() * 1400,
    delay: Math.random() * 280,
    size: 1.2 + Math.random() * 1.4,
    drift: Math.round((Math.random() - 0.5) * 120),
  }));
}

export { REACTIONS };
