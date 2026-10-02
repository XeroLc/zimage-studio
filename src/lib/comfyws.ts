/** ComfyUI 实时事件（WebSocket）单例：进度 / 执行 / 完成 / 出错。
 *  内嵌工作区启用 --enable-cors-header 后，WebSocket 可直接从页面连接。 */
export interface ComfyWsMessage {
  type: string;
  data?: Record<string, unknown>;
}

type Handler = (msg: ComfyWsMessage) => void;
type StatusHandler = (connected: boolean) => void;

class ComfyWs {
  clientId = 'zimg-studio-' + Math.random().toString(36).slice(2, 10);
  #ws: WebSocket | null = null;
  #url = '';
  #handlers = new Set<Handler>();
  #statusHandlers = new Set<StatusHandler>();
  #retryTimer: ReturnType<typeof setTimeout> | null = null;
  #connected = false;

  /** 设置服务地址（http://host:port），变更时自动重连 */
  setUrl(base: string) {
    const ws = base.replace(/^http/, 'ws');
    if (ws === this.#url) return;
    this.#url = ws;
    this.close();
    this.connect();
  }

  connect() {
    if (!this.#url || this.#ws) return;
    try {
      const ws = new WebSocket(`${this.#url}/ws?clientId=${this.clientId}`);
      this.#ws = ws;
      ws.onopen = () => {
        this.#connected = true;
        this.#statusHandlers.forEach((h) => h(true));
      };
      ws.onmessage = (ev) => {
        if (typeof ev.data !== 'string') return; // 二进制预览帧忽略
        try {
          const msg = JSON.parse(ev.data) as ComfyWsMessage;
          this.#handlers.forEach((h) => h(msg));
        } catch {
          /* ignore malformed */
        }
      };
      ws.onclose = () => {
        this.#connected = false;
        this.#statusHandlers.forEach((h) => h(false));
        this.#ws = null;
        this.#scheduleReconnect();
      };
      ws.onerror = () => {
        try {
          ws.close();
        } catch {
          /* ignore */
        }
      };
    } catch {
      this.#scheduleReconnect();
    }
  }

  #scheduleReconnect() {
    if (this.#retryTimer) return;
    this.#retryTimer = setTimeout(() => {
      this.#retryTimer = null;
      this.connect();
    }, 2500);
  }

  close() {
    if (this.#retryTimer) {
      clearTimeout(this.#retryTimer);
      this.#retryTimer = null;
    }
    if (this.#ws) {
      try {
        this.#ws.close();
      } catch {
        /* ignore */
      }
      this.#ws = null;
    }
  }

  get connected() {
    return this.#connected;
  }

  on(h: Handler): () => void {
    this.#handlers.add(h);
    return () => this.#handlers.delete(h);
  }

  onStatus(h: StatusHandler): () => void {
    this.#statusHandlers.add(h);
    return () => this.#statusHandlers.delete(h);
  }
}

export const comfyWs = new ComfyWs();
