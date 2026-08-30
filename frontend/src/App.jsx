import { useEffect, useState } from "react";
import { clearSession, loadSession, saveSession } from "./api.js";
import Login from "./Login.jsx";
import Studio from "./Studio.jsx";

export default function App() {
  const [session, setSession] = useState(() => loadSession());

  useEffect(() => {
    if (session) {
      saveSession(session);
    } else {
      clearSession();
    }
  }, [session]);

  if (!session) {
    return <Login onLogin={setSession} />;
  }

  return <Studio session={session} onLogout={() => setSession(null)} />;
}
