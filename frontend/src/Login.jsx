import { useState } from "react";
import { login } from "./api.js";
import GradientText from "./bits/GradientText.jsx";
import ParticlesField from "./bits/ParticlesField.jsx";
import SpotlightCard from "./bits/SpotlightCard.jsx";

export default function Login({ onLogin }) {
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  async function handleSubmit(event) {
    event.preventDefault();
    setError("");
    setBusy(true);
    try {
      const session = await login(username, password);
      onLogin(session);
    } catch (err) {
      setError(err.message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="login-shell">
      <ParticlesField />
      <div className="login-aurora" aria-hidden="true" />
      <SpotlightCard className="login-card">
        <form onSubmit={handleSubmit}>
          <p className="eyebrow">Sosial Video</p>
          <h1>
            <GradientText>Sign in</GradientText>
          </h1>
          <p className="lede">Use a test account from the README to join a live room.</p>
          <label>
            Username
            <input
              autoComplete="username"
              value={username}
              onChange={(event) => setUsername(event.target.value)}
              required
            />
          </label>
          <label>
            Password
            <input
              type="password"
              autoComplete="current-password"
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              required
            />
          </label>
          {error ? <p className="error">{error}</p> : null}
          <button type="submit" className="glow-button" disabled={busy}>
            {busy ? "Signing in..." : "Sign in"}
          </button>
        </form>
      </SpotlightCard>
    </main>
  );
}
