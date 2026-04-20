"use client";

import { useEffect, useRef } from "react";
import { WS_BASE, type WsMessage } from "./api";

/**
 * Subscribe to `/ws/strategic` and invoke `onMessage` for each tagged delta
 * coming off the server broadcast channel. Reconnects with exponential
 * backoff up to 30s. The callback lives in a ref so it never re-triggers the
 * connection effect.
 */
export function useStrategicWs(onMessage: (msg: WsMessage) => void): void {
  const cbRef = useRef(onMessage);
  cbRef.current = onMessage;

  useEffect(() => {
    let stopped = false;
    let ws: WebSocket | null = null;
    let retryMs = 1000;
    let retryTimer: ReturnType<typeof setTimeout> | null = null;

    function connect() {
      if (stopped) return;
      ws = new WebSocket(`${WS_BASE}/ws/strategic`);

      ws.onopen = () => {
        retryMs = 1000;
      };
      ws.onmessage = (ev) => {
        try {
          const parsed = JSON.parse(ev.data) as WsMessage;
          cbRef.current(parsed);
        } catch (err) {
          console.warn("ws parse failed:", err);
        }
      };
      ws.onclose = () => {
        if (stopped) return;
        retryTimer = setTimeout(connect, retryMs);
        retryMs = Math.min(retryMs * 2, 30_000);
      };
      ws.onerror = () => {
        ws?.close();
      };
    }

    connect();

    return () => {
      stopped = true;
      if (retryTimer) clearTimeout(retryTimer);
      ws?.close();
    };
  }, []);
}
