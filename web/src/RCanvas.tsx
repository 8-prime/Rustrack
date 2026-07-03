import { useEffect, useRef } from "react";
import init, { renderer } from "./pkg/renderer";

export function RCanvas() {
  const ref = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const run = async () => {
      await init();
      if (ref.current !== null) {
        renderer(ref.current);
      }
    };
    run();
  }, []);

  return <canvas ref={ref}></canvas>;
}
