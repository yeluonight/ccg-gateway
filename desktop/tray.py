import threading
from PIL import Image, ImageDraw
import pystray


class TrayIcon:
    def __init__(self, on_show, on_quit):
        self.on_show = on_show
        self.on_quit = on_quit
        self.icon = None
        self._minimize_on_close = True

    @property
    def minimize_on_close(self):
        return self._minimize_on_close

    @minimize_on_close.setter
    def minimize_on_close(self, value):
        self._minimize_on_close = value
        if self.icon:
            self.icon.update_menu()

    def _create_icon_image(self):
        size = 64
        img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
        draw = ImageDraw.Draw(img)
        draw.ellipse([4, 4, size - 4, size - 4], fill="#4A90D9")
        draw.text((size // 2 - 12, size // 2 - 10), "CCG", fill="white")
        return img

    def _toggle_close_behavior(self):
        self._minimize_on_close = not self._minimize_on_close

    def _build_menu(self):
        return pystray.Menu(
            pystray.MenuItem("显示窗口", lambda: self.on_show()),
            pystray.MenuItem(
                "关闭时最小化到托盘",
                lambda: self._toggle_close_behavior(),
                checked=lambda _: self._minimize_on_close,
            ),
            pystray.Menu.SEPARATOR,
            pystray.MenuItem("退出", lambda: self.on_quit()),
        )

    def run(self):
        self.icon = pystray.Icon(
            "ccg-gateway",
            self._create_icon_image(),
            "CCG Gateway",
            menu=self._build_menu(),
        )
        self.icon.run()

    def stop(self):
        if self.icon:
            self.icon.stop()

    def start_in_thread(self):
        thread = threading.Thread(target=self.run, daemon=True)
        thread.start()
        return thread
