import { useEffect, useState } from "react";
import { confirmAccount, login, register } from "./api.js";
import GradientText from "./bits/GradientText.jsx";
import ParticlesField from "./bits/ParticlesField.jsx";
import SpotlightCard from "./bits/SpotlightCard.jsx";

const VERIFY_MESSAGES = {
  expired: "This sign-up expired. Register a new account.",
  invalid: "This confirmation link is not valid.",
};

export default function Login({ onLogin }) {
  const [mode, setMode] = useState("signin");
  const [username, setUsername] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [code, setCode] = useState("");
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);
  const [confirmingLink, setConfirmingLink] = useState(
    () => typeof window !== "undefined" && new URLSearchParams(window.location.search).has("verify")
  );

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const token = params.get("verify");
    const verified = params.get("verified");
    const verifyError = params.get("verify_error");

    function clearQuery() {
      window.history.replaceState({}, "", window.location.pathname);
    }

    if (token) {
      setConfirmingLink(true);
      confirmAccount({ token })
        .then((data) => {
          setNotice(data.message || "Account confirmed. You can sign in.");
          setMode("signin");
        })
        .catch((err) => {
          setError(err.message);
          if (/expired/i.test(err.message)) {
            setMode("signup");
          }
        })
        .finally(() => {
          setConfirmingLink(false);
          clearQuery();
        });
      return;
    }

    if (verified) {
      setNotice("Account confirmed. You can sign in.");
      setMode("signin");
      clearQuery();
      return;
    }

    if (verifyError) {
      setError(VERIFY_MESSAGES[verifyError] || VERIFY_MESSAGES.invalid);
      setMode(verifyError === "expired" ? "signup" : "signin");
      clearQuery();
    }
  }, []);

  async function handleSignIn(event) {
    event.preventDefault();
    setError("");
    setNotice("");
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

  async function handleSignUp(event) {
    event.preventDefault();
    setError("");
    setNotice("");
    if (password !== confirmPassword) {
      setError("Passwords do not match.");
      return;
    }
    setBusy(true);
    try {
      const data = await register(username, email, password);
      setNotice(data.message);
      setMode("confirm");
      setPassword("");
      setConfirmPassword("");
    } catch (err) {
      setError(err.message);
    } finally {
      setBusy(false);
    }
  }

  async function handleConfirm(event) {
    event.preventDefault();
    setError("");
    setNotice("");
    setBusy(true);
    try {
      const data = await confirmAccount({ code });
      setNotice(data.message);
      setMode("signin");
      setCode("");
    } catch (err) {
      setError(err.message);
    } finally {
      setBusy(false);
    }
  }

  const title = mode === "signup" ? "Register account" : mode === "confirm" ? "Confirm email" : "Sign in";

  return (
    <main className="login-shell">
      <ParticlesField />
      <div className="login-aurora" aria-hidden="true" />
      <SpotlightCard className="login-card">
        <p className="eyebrow">Sosial Video</p>
        <h1>
          <GradientText>{title}</GradientText>
        </h1>
        {confirmingLink ? <p className="lede">Confirming your account...</p> : null}

        {mode !== "confirm" && !confirmingLink ? (
          <div className="auth-tabs" role="tablist" aria-label="Account">
            <button
              type="button"
              role="tab"
              className={mode === "signin" ? "active" : ""}
              aria-selected={mode === "signin"}
              onClick={() => {
                setMode("signin");
                setError("");
              }}
            >
              Sign in
            </button>
            <button
              type="button"
              role="tab"
              className={mode === "signup" ? "active" : ""}
              aria-selected={mode === "signup"}
              onClick={() => {
                setMode("signup");
                setError("");
              }}
            >
              Register account
            </button>
          </div>
        ) : null}

        {mode === "signin" && !confirmingLink ? (
          <form onSubmit={handleSignIn}>
            <p className="lede">Sign in with an active account, or create one and confirm it by email.</p>
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
            {notice ? <p className="success">{notice}</p> : null}
            {error ? <p className="error">{error}</p> : null}
            <button type="submit" className="glow-button" disabled={busy}>
              {busy ? "Signing in..." : "Sign in"}
            </button>
          </form>
        ) : null}

        {mode === "signup" ? (
          <form onSubmit={handleSignUp}>
            <p className="lede">
              We send a confirmation link to your email. Click it within 1 day to activate your account.
            </p>
            <label>
              Username
              <input
                autoComplete="username"
                value={username}
                onChange={(event) => setUsername(event.target.value)}
                minLength={3}
                maxLength={32}
                required
              />
            </label>
            <label>
              Email
              <input
                type="email"
                autoComplete="email"
                value={email}
                onChange={(event) => setEmail(event.target.value)}
                required
              />
            </label>
            <label>
              Password
              <input
                type="password"
                autoComplete="new-password"
                value={password}
                onChange={(event) => setPassword(event.target.value)}
                minLength={8}
                required
              />
            </label>
            <label>
              Confirm password
              <input
                type="password"
                autoComplete="new-password"
                value={confirmPassword}
                onChange={(event) => setConfirmPassword(event.target.value)}
                minLength={8}
                required
              />
            </label>
            {error ? <p className="error">{error}</p> : null}
            <button type="submit" className="glow-button" disabled={busy}>
              {busy ? "Sending..." : "Register account"}
            </button>
          </form>
        ) : null}

        {mode === "confirm" ? (
          <form onSubmit={handleConfirm}>
            <p className="lede">
              {notice || "Check your inbox and click the confirmation link within 1 day."} You can also
              enter the confirmation code from the email.
            </p>
            <label>
              Confirmation code
              <input
                autoComplete="one-time-code"
                value={code}
                onChange={(event) => setCode(event.target.value.toUpperCase())}
                minLength={8}
                maxLength={8}
                required
              />
            </label>
            {error ? <p className="error">{error}</p> : null}
            <button type="submit" className="glow-button" disabled={busy}>
              {busy ? "Confirming..." : "Confirm account"}
            </button>
            <button
              type="button"
              className="ghost"
              onClick={() => {
                setMode("signin");
                setError("");
              }}
            >
              Back to sign in
            </button>
          </form>
        ) : null}
      </SpotlightCard>
    </main>
  );
}
