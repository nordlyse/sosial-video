import { useEffect, useRef, useState } from "react";
import { fetchComments, postComment } from "./api.js";

const EXPANDED_KEY = "sosial-video-comments-expanded";

export default function CommentsPanel({ token, targetUser, currentUser, compact }) {
  const [comments, setComments] = useState([]);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [expanded, setExpanded] = useState(readExpanded);
  const [composer, setComposer] = useState(null);
  const listRef = useRef(null);
  const isOwner = currentUser?.id === targetUser?.id;

  useEffect(() => {
    if (!token || !targetUser?.id) {
      return undefined;
    }
    let cancelled = false;
    async function refresh() {
      try {
        const list = await fetchComments(token, targetUser.id);
        if (!cancelled) {
          setComments(list);
          setError("");
        }
      } catch (err) {
        if (!cancelled) {
          setError(err.message);
        }
      }
    }
    refresh();
    const timer = setInterval(refresh, 1500);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [token, targetUser?.id]);

  useEffect(() => {
    const node = listRef.current;
    if (node && expanded) {
      node.scrollTop = node.scrollHeight;
    }
  }, [comments, expanded]);

  function toggleExpanded() {
    setExpanded((prev) => {
      const next = !prev;
      writeExpanded(next);
      return next;
    });
  }

  async function handleSubmit(event) {
    event.preventDefault();
    const body = draft.trim();
    if (!body || busy) {
      return;
    }
    setBusy(true);
    try {
      const created = await postComment(token, targetUser.id, body, {
        parentId: composer?.parent.id,
        isPrivate: composer?.mode === "private",
      });
      setComments((prev) => [...prev, created]);
      setDraft("");
      setComposer(null);
      setError("");
    } catch (err) {
      setError(err.message);
    } finally {
      setBusy(false);
    }
  }

  const roots = comments.filter((item) => !item.parent_id);

  if (!expanded) {
    return (
      <div className="comments-panel collapsed">
        <button
          type="button"
          className="comments-toggle"
          onClick={toggleExpanded}
          aria-expanded="false"
          title="Show comments"
        >
          <span>Comments</span>
          {comments.length > 0 ? <em>{comments.length}</em> : null}
        </button>
      </div>
    );
  }

  return (
    <div className={`comments-panel expanded ${compact ? "compact" : ""}`}>
      <div className="comments-head">
        <div>
          <h2>Comments</h2>
          <p className="hint">Newest comments appear at the bottom.</p>
        </div>
        <button type="button" className="ghost tiny" onClick={toggleExpanded} aria-expanded="true">
          Hide
        </button>
      </div>
      <div className="comment-list" ref={listRef}>
        {roots.length === 0 ? <p className="hint">No comments yet.</p> : null}
        {roots.map((comment) => (
          <CommentThread
            key={comment.id}
            comment={comment}
            comments={comments}
            isOwner={isOwner}
            currentUser={currentUser}
            onPrivate={(item) => {
              setComposer({ mode: "private", parent: item });
              setDraft("");
            }}
            onPublic={(item) => {
              setComposer({ mode: "public", parent: item });
              setDraft("");
            }}
          />
        ))}
      </div>
      <form className="comment-form" onSubmit={handleSubmit}>
        {composer ? (
          <p className={`composer-banner ${composer.mode}`}>
            {composer.mode === "private"
              ? `Private reply to ${composer.parent.from.username}`
              : `Public reply to ${composer.parent.from.username}`}
            <button type="button" className="tiny ghost" onClick={() => setComposer(null)}>
              Cancel
            </button>
          </p>
        ) : null}
        <textarea
          value={draft}
          maxLength={280}
          rows={compact ? 2 : 3}
          placeholder={placeholderFor(composer)}
          onChange={(event) => setDraft(event.target.value)}
        />
        <button type="submit" className="glow-button" disabled={busy || !draft.trim()}>
          {busy ? "Sending..." : composer?.mode === "private" ? "Send privately" : "Send"}
        </button>
      </form>
      {error ? <p className="error">{error}</p> : null}
    </div>
  );
}

function CommentThread({ comment, comments, isOwner, currentUser, onPrivate, onPublic }) {
  const replies = comments.filter((item) => item.parent_id === comment.id);
  return (
    <div className="comment-thread">
      <CommentItem
        comment={comment}
        isOwner={isOwner}
        currentUser={currentUser}
        onPrivate={onPrivate}
        onPublic={onPublic}
      />
      {replies.length > 0 ? (
        <div className="comment-replies">
          {replies.map((reply) => (
            <CommentThread
              key={reply.id}
              comment={reply}
              comments={comments}
              isOwner={isOwner}
              currentUser={currentUser}
              onPrivate={onPrivate}
              onPublic={onPublic}
            />
          ))}
        </div>
      ) : null}
    </div>
  );
}

function CommentItem({ comment, isOwner, currentUser, onPrivate, onPublic }) {
  const mine = comment.from.id === currentUser?.id;
  return (
    <article
      className={`comment-item ${comment.is_private ? "private" : ""} ${comment.parent_id ? "nested" : ""}`}
      onClick={
        isOwner && !mine
          ? () => onPrivate(comment)
          : undefined
      }
      title={isOwner && !mine ? "Click to reply privately to this person" : undefined}
    >
      <strong>
        {comment.from.username}
        {comment.is_private ? <span className="private-tag">Private</span> : null}
      </strong>
      <time dateTime={comment.created_at}>{formatTime(comment.created_at)}</time>
      <p>
        {comment.reply_to && (comment.is_private || comment.parent_id) ? (
          <span className="reply-to">@{comment.reply_to.username} </span>
        ) : null}
        {comment.body}
      </p>
      <div className="comment-actions" onClick={(event) => event.stopPropagation()}>
        <button type="button" className="tiny ghost" onClick={() => onPublic(comment)}>
          Reply
        </button>
        {isOwner && !mine ? (
          <button type="button" className="tiny ghost" onClick={() => onPrivate(comment)}>
            Private reply
          </button>
        ) : null}
      </div>
    </article>
  );
}

function placeholderFor(composer) {
  if (composer?.mode === "private") {
    return `Private message to ${composer.parent.from.username}...`;
  }
  if (composer?.mode === "public") {
    return `Reply for everyone...`;
  }
  return "Write a comment...";
}

function readExpanded() {
  try {
    return sessionStorage.getItem(EXPANDED_KEY) !== "0";
  } catch {
    return true;
  }
}

function writeExpanded(value) {
  try {
    sessionStorage.setItem(EXPANDED_KEY, value ? "1" : "0");
  } catch {
    // Ignore storage failures.
  }
}

function formatTime(value) {
  if (!value) {
    return "";
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return "";
  }
  return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}
