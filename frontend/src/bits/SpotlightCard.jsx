import { useRef } from "react";

export default function SpotlightCard({ children, className = "", spotlightColor = "rgba(82, 39, 255, 0.28)" }) {
  const ref = useRef(null);

  function handleMouseMove(event) {
    const node = ref.current;
    if (!node) {
      return;
    }
    const rect = node.getBoundingClientRect();
    node.style.setProperty("--mouse-x", `${event.clientX - rect.left}px`);
    node.style.setProperty("--mouse-y", `${event.clientY - rect.top}px`);
    node.style.setProperty("--spotlight-color", spotlightColor);
  }

  return (
    <div ref={ref} className={`spotlight-card ${className}`} onMouseMove={handleMouseMove}>
      {children}
    </div>
  );
}
