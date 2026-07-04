import { useEffect, useRef } from "react";
import init, { renderer } from "./pkg/renderer";

export function RCanvas() {
  const ref = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const run = async () => {
      await init();
      if (ref.current !== null) {
        const canvas = ref.current;
        const dpr = window.devicePixelRatio || 1;
        canvas.width = Math.floor(canvas.clientWidth * dpr);
        canvas.height = Math.floor(canvas.clientHeight * dpr);
        renderer(canvas);
      }
    };
    run();
  }, []);

  return <canvas className="h-full w-full" ref={ref}></canvas>;
}
