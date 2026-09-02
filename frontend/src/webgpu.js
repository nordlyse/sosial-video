export async function drawVideoWithWebGpu(canvas, video, onReady) {
  if (!navigator.gpu) {
    return false;
  }
  const ua = navigator.userAgent || "";
  if (/iPhone|iPad|iPod/i.test(ua) || (navigator.platform === "MacIntel" && navigator.maxTouchPoints > 1)) {
    return false;
  }
  const adapter = await navigator.gpu.requestAdapter();
  if (!adapter) {
    return false;
  }
  const device = await adapter.requestDevice();
  const context = canvas.getContext("webgpu");
  if (!context) {
    return false;
  }
  const format = navigator.gpu.getPreferredCanvasFormat();
  const usage = GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_DST;
  let configuredWidth = 0;
  let configuredHeight = 0;
  let stopped = false;
  let ready = false;

  const configure = (width, height) => {
    canvas.width = width;
    canvas.height = height;
    context.configure({
      device,
      format,
      usage,
      alphaMode: "opaque",
    });
    configuredWidth = width;
    configuredHeight = height;
  };

  const frame = () => {
    if (stopped) {
      return;
    }
    const width = video.videoWidth;
    const height = video.videoHeight;
    if (width > 0 && height > 0 && (width !== configuredWidth || height !== configuredHeight)) {
      configure(width, height);
    }
    if (video.readyState >= 2 && configuredWidth > 0 && configuredHeight > 0) {
      try {
        device.queue.copyExternalImageToTexture(
          { source: video },
          { texture: context.getCurrentTexture() },
          { width: configuredWidth, height: configuredHeight }
        );
        if (!ready) {
          ready = true;
          onReady?.();
        }
      } catch (err) {
        console.warn("WebGPU video copy failed", err);
      }
    }
    requestAnimationFrame(frame);
  };

  if (video.videoWidth > 0 && video.videoHeight > 0) {
    configure(video.videoWidth, video.videoHeight);
  }
  requestAnimationFrame(frame);
  return () => {
    stopped = true;
  };
}
