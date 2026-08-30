const EMOJIS = ["🦊", "🐼", "🐸", "🦁", "🐙", "🦄", "🐝", "🐧", "🐨", "🐯", "🦋", "🐬"];

export function userEmoji(username) {
  let hash = 0;
  for (const character of username || "") {
    hash = (hash * 33 + character.charCodeAt(0)) >>> 0;
  }
  return EMOJIS[hash % EMOJIS.length];
}

export function streamHasVideo(stream) {
  return Boolean(
    stream
      ?.getVideoTracks()
      .some((track) => track.enabled && track.readyState === "live")
  );
}
