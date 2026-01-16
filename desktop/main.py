import os
import sys
import threading
import time

# Set desktop mode before importing app
os.environ["DESKTOP_MODE"] = "1"

# Add backend to path (for development mode)
backend_path = os.path.join(os.path.dirname(os.path.dirname(__file__)), "backend")
if os.path.exists(backend_path):
    sys.path.insert(0, backend_path)

import uvicorn
import webview
from desktop.tray import TrayIcon
from app.main import app as fastapi_app


class Server:
    def __init__(self, host="127.0.0.1", port=7788):
        self.host = host
        self.port = port
        self.server = None

    def run(self):
        config = uvicorn.Config(
            fastapi_app,
            host=self.host,
            port=self.port,
            log_level="info",
        )
        self.server = uvicorn.Server(config)
        self.server.run()

    def start_in_thread(self):
        thread = threading.Thread(target=self.run, daemon=True)
        thread.start()
        return thread


class App:
    def __init__(self):
        self.window = None
        self.server = Server()
        self.tray = TrayIcon(on_show=self.show_window, on_quit=self.quit)

    def show_window(self):
        if self.window:
            self.window.show()

    def hide_window(self):
        if self.window:
            self.window.hide()

    def on_closing(self):
        if self.tray.minimize_on_close:
            self.hide_window()
            return False  # Prevent window destruction
        return True  # Allow window destruction

    def quit(self):
        self.tray.stop()
        if self.window:
            self.window.destroy()

    def run(self):
        # Start FastAPI server
        self.server.start_in_thread()
        time.sleep(1)  # Wait for server to start

        # Start tray icon
        self.tray.start_in_thread()

        # Create webview window
        self.window = webview.create_window(
            "CCG Gateway",
            f"http://{self.server.host}:{self.server.port}",
            width=1200,
            height=800,
        )
        self.window.events.closing += self.on_closing

        # Start webview (blocks until window is closed)
        webview.start()


def main():
    app = App()
    app.run()


if __name__ == "__main__":
    main()
