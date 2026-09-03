const LANGUAGE_BY_PRIMARY = {
  ar: "ar-SA",
  de: "de-DE",
  en: "en-US",
  es: "es-ES",
  fr: "fr-FR",
  it: "it-IT",
  ja: "ja-JP",
  pt: "pt-BR",
  tr: "tr-TR",
  zh: "zh-CN",
};

export function speechRecognitionSupported() {
  return Boolean(
    typeof window !== "undefined" &&
      (window.SpeechRecognition || window.webkitSpeechRecognition)
  );
}

export function recognitionLanguage() {
  if (typeof navigator === "undefined") {
    return "en-US";
  }
  const raw = navigator.language || "en-US";
  if (/^[a-z]{2}-[A-Z]{2}$/.test(raw) || /^[a-z]{2}-[a-z]{2}$/i.test(raw)) {
    return raw;
  }
  const primary = raw.split("-")[0].toLowerCase();
  return LANGUAGE_BY_PRIMARY[primary] || "en-US";
}

export function startBroadcastSpeechLog({ onFinal }) {
  const SpeechRecognition = window.SpeechRecognition || window.webkitSpeechRecognition;
  if (!SpeechRecognition || typeof onFinal !== "function") {
    return () => {};
  }

  const recognition = new SpeechRecognition();
  recognition.continuous = true;
  recognition.interimResults = true;
  recognition.lang = recognitionLanguage();

  let stopped = false;
  let restartTimer = 0;

  recognition.onresult = (event) => {
    let finalText = "";
    for (let i = event.resultIndex; i < event.results.length; i += 1) {
      const piece = event.results[i][0]?.transcript || "";
      if (event.results[i].isFinal && piece.trim()) {
        finalText += `${piece} `;
      }
    }
    const body = finalText.trim();
    if (body) {
      onFinal(body);
    }
  };

  recognition.onerror = (event) => {
    if (event.error === "not-allowed" || event.error === "service-not-allowed") {
      stopped = true;
    }
  };

  recognition.onend = () => {
    if (stopped) {
      return;
    }
    restartTimer = window.setTimeout(() => {
      if (stopped) {
        return;
      }
      try {
        recognition.start();
      } catch {
        // Chrome throws if start() is called while it is already running.
      }
    }, 250);
  };

  try {
    recognition.start();
  } catch {
    return () => {};
  }

  return () => {
    stopped = true;
    window.clearTimeout(restartTimer);
    try {
      recognition.stop();
    } catch {
      // Already stopped.
    }
  };
}
