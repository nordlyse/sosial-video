import { useEffect, useRef } from "react";

export default function ParticlesField({ className = "" }) {
  const canvasRef = useRef(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) {
      return undefined;
    }
    const context = canvas.getContext("2d");
    const dots = Array.from({ length: 70 }, () => ({
      x: Math.random(),
      y: Math.random(),
      vx: (Math.random() - 0.5) * 0.00035,
      vy: (Math.random() - 0.5) * 0.00035,
      size: Math.random() * 1.8 + 0.4,
    }));
    let frame = 0;
    let running = true;

    function resize() {
      const ratio = window.devicePixelRatio || 1;
      canvas.width = canvas.clientWidth * ratio;
      canvas.height = canvas.clientHeight * ratio;
      context.setTransform(ratio, 0, 0, ratio, 0, 0);
    }

    function draw() {
      if (!running) {
        return;
      }
      const width = canvas.clientWidth;
      const height = canvas.clientHeight;
      context.clearRect(0, 0, width, height);
      frame += 1;
      for (const dot of dots) {
        dot.x += dot.vx;
        dot.y += dot.vy;
        if (dot.x < 0 || dot.x > 1) {
          dot.vx *= -1;
        }
        if (dot.y < 0 || dot.y > 1) {
          dot.vy *= -1;
        }
      }
      for (let i = 0; i < dots.length; i += 1) {
        for (let j = i + 1; j < dots.length; j += 1) {
          const dx = dots[i].x - dots[j].x;
          const dy = dots[i].y - dots[j].y;
          const dist = Math.hypot(dx * width, dy * height);
          if (dist < 120) {
            context.strokeStyle = `rgba(167, 139, 250, ${0.16 - dist / 900})`;
            context.beginPath();
            context.moveTo(dots[i].x * width, dots[i].y * height);
            context.lineTo(dots[j].x * width, dots[j].y * height);
            context.stroke();
          }
        }
      }
      for (const dot of dots) {
        const pulse = 0.45 + Math.sin((frame + dot.size * 40) / 40) * 0.25;
        context.fillStyle = `rgba(255, 255, 255, ${pulse})`;
        context.beginPath();
        context.arc(dot.x * width, dot.y * height, dot.size, 0, Math.PI * 2);
        context.fill();
      }
      requestAnimationFrame(draw);
    }

    resize();
    window.addEventListener("resize", resize);
    draw();
    return () => {
      running = false;
      window.removeEventListener("resize", resize);
    };
  }, []);

  return <canvas ref={canvasRef} className={`particles-field ${className}`} aria-hidden="true" />;
}
