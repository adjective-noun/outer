import type { ClientMessage, ServerMessage } from './types';

export type MessageHandler = (message: ServerMessage) => void;

export class WebSocketClient {
	private ws: WebSocket | null = null;
	private url: string;
	private handlers: Set<MessageHandler> = new Set();
	private reconnectAttempts = 0;
	private maxReconnectAttempts = 5;
	private reconnectDelay = 1000;
	private connected = false;
	private messageQueue: ClientMessage[] = [];

	constructor(url?: string) {
		// Default to same host, /ws path
		this.url = url || `${location.protocol === 'https:' ? 'wss:' : 'ws:'}//${location.host}/ws`;
	}

	connect(): Promise<void> {
		return new Promise((resolve, reject) => {
			try {
				this.ws = new WebSocket(this.url);

				this.ws.onopen = () => {
					this.connected = true;
					this.reconnectAttempts = 0;
					// Flush queued messages
					while (this.messageQueue.length > 0) {
						const msg = this.messageQueue.shift();
						if (msg) this.send(msg);
					}
					resolve();
				};

				this.ws.onclose = () => {
					this.connected = false;
					this.attemptReconnect();
				};

				this.ws.onerror = (event) => {
					console.error('WebSocket error:', event);
					if (!this.connected) {
						reject(new Error('WebSocket connection failed'));
					}
				};

				this.ws.onmessage = (event) => {
					try {
						const message: ServerMessage = JSON.parse(event.data);
						this.handlers.forEach((handler) => handler(message));
					} catch (e) {
						console.error('Failed to parse message:', e);
					}
				};
			} catch (e) {
				reject(e);
			}
		});
	}

	private attemptReconnect() {
		if (this.reconnectAttempts >= this.maxReconnectAttempts) {
			console.error('Max reconnection attempts reached');
			return;
		}

		this.reconnectAttempts++;
		const delay = this.reconnectDelay * Math.pow(2, this.reconnectAttempts - 1);

		setTimeout(() => {
			console.log(`Reconnecting... attempt ${this.reconnectAttempts}`);
			this.connect().catch(() => {});
		}, delay);
	}

	send(message: ClientMessage) {
		if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
			this.messageQueue.push(message);
			return;
		}
		this.ws.send(JSON.stringify(message));
	}

	subscribe(handler: MessageHandler): () => void {
		this.handlers.add(handler);
		return () => this.handlers.delete(handler);
	}

	disconnect() {
		if (this.ws) {
			this.ws.close();
			this.ws = null;
		}
	}

	get isConnected(): boolean {
		return this.connected;
	}
}

// Singleton instance
let client: WebSocketClient | null = null;

export function getWebSocketClient(): WebSocketClient {
	if (!client) {
		client = new WebSocketClient();
	}
	return client;
}
