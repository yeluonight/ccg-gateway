import os
import json
import platform
import shutil
import hashlib
from pathlib import Path
from typing import Optional
from pydantic import BaseModel


class SessionInfo(BaseModel):
    session_id: str
    size: int
    mtime: float
    first_message: str = ""
    git_branch: str = ""
    summary: str = ""


class ProjectInfo(BaseModel):
    name: str
    display_name: str
    full_path: str
    session_count: int
    total_size: int
    last_modified: float


class PaginatedProjects(BaseModel):
    items: list[ProjectInfo]
    total: int
    page: int
    page_size: int


class PaginatedSessions(BaseModel):
    items: list[SessionInfo]
    total: int
    page: int
    page_size: int


class SessionService:
    CLI_DIRS = {
        "claude_code": ".claude",
        "codex": ".codex",
        "gemini": ".gemini"
    }

    # Gemini path mapping cache
    _gemini_path_cache: dict = {}
    _gemini_cache_time: float = 0
    _CACHE_TTL = 60  # seconds

    def __init__(self):
        self.home_dir = Path.home()

    def _get_base_dir(self, cli_type: str) -> Path:
        cli_dir = self.CLI_DIRS.get(cli_type, ".claude")
        return self.home_dir / cli_dir

    def _get_projects_dir(self, cli_type: str) -> Path:
        """Get projects directory for Claude Code only."""
        return self._get_base_dir(cli_type) / "projects"

    def _get_codex_sessions_dir(self) -> Path:
        return self._get_base_dir("codex") / "sessions"

    def _get_gemini_tmp_dir(self) -> Path:
        return self._get_base_dir("gemini") / "tmp"

    def _get_path_hash(self, path: str) -> str:
        """Calculate SHA256 hash of a path (same as Gemini CLI)."""
        return hashlib.sha256(path.encode()).hexdigest()

    def _build_gemini_path_mapping(self, target_hashes: set) -> dict:
        """Build hash -> path mapping for Gemini projects using rainbow table method."""
        import time
        now = time.time()

        # Check cache
        if SessionService._gemini_path_cache and (now - SessionService._gemini_cache_time) < SessionService._CACHE_TTL:
            return SessionService._gemini_path_cache

        results = {}

        # Define search paths with max depth
        search_paths = [
            (self.home_dir, 0),
            (self.home_dir / "Desktop", 4),
            (self.home_dir / "Documents", 4),
            (self.home_dir / "Downloads", 3),
            (self.home_dir / "Projects", 4),
            (self.home_dir / "Code", 4),
            (self.home_dir / "workspace", 4),
            (self.home_dir / "dev", 4),
            (self.home_dir / "src", 4),
            (self.home_dir / "work", 4),
            (self.home_dir / "repos", 4),
            (self.home_dir / "github", 4),
        ]

        # Windows specific paths
        if platform.system() == "Windows":
            for drive in ["C:", "D:", "E:", "F:"]:
                drive_path = Path(drive + "/")
                if drive_path.exists():
                    search_paths.extend([
                        (drive_path / "Projects", 4),
                        (drive_path / "Code", 4),
                        (drive_path / "workspace", 4),
                        (drive_path / "dev", 4),
                        (drive_path / "my-develop", 4),
                    ])

        def scan_dir(dir_path: Path, max_depth: int, current_depth: int = 0):
            if current_depth > max_depth or len(results) >= len(target_hashes):
                return

            # Calculate hash for current directory
            path_str = str(dir_path)
            path_hash = self._get_path_hash(path_str)
            if path_hash in target_hashes and path_hash not in results:
                results[path_hash] = path_str

            if len(results) >= len(target_hashes):
                return

            # Scan subdirectories
            try:
                for item in dir_path.iterdir():
                    if not item.is_dir():
                        continue
                    # Skip hidden and common irrelevant directories
                    if item.name.startswith('.') or item.name in ('node_modules', 'venv', '__pycache__', 'Library', 'Applications'):
                        continue
                    scan_dir(item, max_depth, current_depth + 1)
                    if len(results) >= len(target_hashes):
                        return
            except (PermissionError, OSError):
                pass

        for search_path, depth in search_paths:
            if search_path.exists():
                scan_dir(search_path, depth)
            if len(results) >= len(target_hashes):
                break

        SessionService._gemini_path_cache = results
        SessionService._gemini_cache_time = now

        return results

    def _get_gemini_project_path(self, project_hash: str, all_hashes: set) -> Optional[str]:
        """Get the real path for a Gemini project hash."""
        mapping = self._build_gemini_path_mapping(all_hashes)
        return mapping.get(project_hash)

    def _decode_project_name(self, encoded_name: str) -> tuple[str, str]:
        """Decode project directory name to display name and full path."""
        if encoded_name.startswith("-"):
            parts = encoded_name[1:].split("-")
            if platform.system() == "Windows":
                if len(parts) >= 2 and len(parts[0]) == 1:
                    drive = parts[0].upper()
                    path_parts = parts[1:]
                    full_path = f"{drive}:\\" + "\\".join(path_parts)
                    display_name = path_parts[-1] if path_parts else encoded_name
                    return display_name, full_path
            else:
                full_path = "/" + "/".join(parts)
                display_name = parts[-1] if parts else encoded_name
                return display_name, full_path
        return encoded_name, encoded_name

    def list_projects(self, cli_type: str, page: int = 1, page_size: int = 20) -> PaginatedProjects:
        """List projects with pagination."""
        if cli_type == "codex":
            return self._list_codex_projects(page, page_size)
        elif cli_type == "gemini":
            return self._list_gemini_projects(page, page_size)
        else:
            return self._list_claude_projects(page, page_size)

    def _list_claude_projects(self, page: int, page_size: int) -> PaginatedProjects:
        """List Claude Code projects."""
        projects_dir = self._get_projects_dir("claude_code")
        if not projects_dir.exists():
            return PaginatedProjects(items=[], total=0, page=page, page_size=page_size)

        project_dirs = []
        for item in projects_dir.iterdir():
            if not item.is_dir():
                continue
            try:
                stat = item.stat()
                project_dirs.append((item, stat.st_mtime))
            except OSError:
                continue

        project_dirs.sort(key=lambda x: x[1], reverse=True)
        total = len(project_dirs)

        start = (page - 1) * page_size
        end = start + page_size
        page_dirs = project_dirs[start:end]

        projects = []
        for item, _ in page_dirs:
            display_name, full_path = self._decode_project_name(item.name)
            sessions = list(item.glob("*.jsonl"))
            sessions = [s for s in sessions if not s.name.startswith("agent-")]

            if not sessions:
                total -= 1
                continue

            total_size = sum(s.stat().st_size for s in sessions)
            last_modified = max(s.stat().st_mtime for s in sessions)

            projects.append(ProjectInfo(
                name=item.name,
                display_name=display_name,
                full_path=full_path,
                session_count=len(sessions),
                total_size=total_size,
                last_modified=last_modified
            ))

        return PaginatedProjects(items=projects, total=total, page=page, page_size=page_size)

    def _list_codex_projects(self, page: int, page_size: int) -> PaginatedProjects:
        """List Codex projects (grouped by cwd from session files)."""
        sessions_dir = self._get_codex_sessions_dir()
        if not sessions_dir.exists():
            return PaginatedProjects(items=[], total=0, page=page, page_size=page_size)

        # Collect all session files and group by cwd
        project_map = {}  # cwd -> list of (file, stat)
        for session_file in sessions_dir.rglob("rollout-*.jsonl"):
            try:
                stat = session_file.stat()
                cwd = self._extract_codex_cwd(session_file)
                if cwd:
                    if cwd not in project_map:
                        project_map[cwd] = []
                    project_map[cwd].append((session_file, stat))
            except OSError:
                continue

        # Build project list
        projects_data = []
        for cwd, files in project_map.items():
            total_size = sum(s.st_size for _, s in files)
            last_modified = max(s.st_mtime for _, s in files)
            display_name = Path(cwd).name if cwd else "Unknown"
            projects_data.append((cwd, display_name, len(files), total_size, last_modified))

        projects_data.sort(key=lambda x: x[4], reverse=True)
        total = len(projects_data)

        start = (page - 1) * page_size
        end = start + page_size
        page_data = projects_data[start:end]

        projects = []
        for cwd, display_name, session_count, total_size, last_modified in page_data:
            projects.append(ProjectInfo(
                name=cwd,
                display_name=display_name,
                full_path=cwd,
                session_count=session_count,
                total_size=total_size,
                last_modified=last_modified
            ))

        return PaginatedProjects(items=projects, total=total, page=page, page_size=page_size)

    def _list_gemini_projects(self, page: int, page_size: int) -> PaginatedProjects:
        """List Gemini projects (from tmp/<hash>/chats directories)."""
        tmp_dir = self._get_gemini_tmp_dir()
        if not tmp_dir.exists():
            return PaginatedProjects(items=[], total=0, page=page, page_size=page_size)

        # Collect all project hashes first
        project_dirs = []
        all_hashes = set()
        for item in tmp_dir.iterdir():
            if not item.is_dir():
                continue
            # Filter valid project hash directories (64 hex chars)
            if not (len(item.name) == 64 and all(c in '0123456789abcdef' for c in item.name)):
                continue
            chats_dir = item / "chats"
            if not chats_dir.exists():
                continue
            try:
                stat = item.stat()
                project_dirs.append((item, stat.st_mtime))
                all_hashes.add(item.name)
            except OSError:
                continue

        project_dirs.sort(key=lambda x: x[1], reverse=True)
        total = len(project_dirs)

        start = (page - 1) * page_size
        end = start + page_size
        page_dirs = project_dirs[start:end]

        # Build path mapping for all hashes
        path_mapping = self._build_gemini_path_mapping(all_hashes)

        projects = []
        for item, _ in page_dirs:
            chats_dir = item / "chats"
            sessions = list(chats_dir.glob("session-*.json"))

            if not sessions:
                total -= 1
                continue

            total_size = sum(s.stat().st_size for s in sessions)
            last_modified = max(s.stat().st_mtime for s in sessions)

            # Try to get project path from rainbow table
            real_path = path_mapping.get(item.name)
            if real_path:
                display_name = Path(real_path).name
                full_path = real_path
            else:
                display_name = f"Project {item.name[:8]}"
                full_path = item.name

            projects.append(ProjectInfo(
                name=item.name,
                display_name=display_name,
                full_path=full_path,
                session_count=len(sessions),
                total_size=total_size,
                last_modified=last_modified
            ))

        return PaginatedProjects(items=projects, total=total, page=page, page_size=page_size)

    def _extract_codex_cwd(self, file_path: Path) -> Optional[str]:
        """Extract cwd from Codex session file."""
        try:
            with open(file_path, "r", encoding="utf-8") as f:
                for line in f:
                    line = line.strip()
                    if not line:
                        continue
                    try:
                        data = json.loads(line)
                        if data.get("type") == "session_meta":
                            payload = data.get("payload", {})
                            return payload.get("cwd")
                    except json.JSONDecodeError:
                        continue
        except Exception:
            pass
        return None

    def list_sessions(self, cli_type: str, project_name: str, page: int = 1, page_size: int = 20) -> PaginatedSessions:
        """List sessions with pagination."""
        if cli_type == "codex":
            return self._list_codex_sessions(project_name, page, page_size)
        elif cli_type == "gemini":
            return self._list_gemini_sessions(project_name, page, page_size)
        else:
            return self._list_claude_sessions(project_name, page, page_size)

    def _list_claude_sessions(self, project_name: str, page: int, page_size: int) -> PaginatedSessions:
        """List Claude Code sessions."""
        projects_dir = self._get_projects_dir("claude_code")
        project_dir = projects_dir / project_name

        if not project_dir.exists():
            return PaginatedSessions(items=[], total=0, page=page, page_size=page_size)

        session_files = []
        for session_file in project_dir.glob("*.jsonl"):
            if session_file.name.startswith("agent-"):
                continue
            try:
                stat = session_file.stat()
                session_files.append((session_file, stat))
            except OSError:
                continue

        session_files.sort(key=lambda x: x[1].st_mtime, reverse=True)
        total = len(session_files)

        start = (page - 1) * page_size
        end = start + page_size
        page_files = session_files[start:end]

        sessions = []
        for session_file, stat in page_files:
            session_id = session_file.stem
            info = self._parse_claude_session_info(session_file)
            sessions.append(SessionInfo(
                session_id=session_id,
                size=stat.st_size,
                mtime=stat.st_mtime,
                first_message=info.get("first_message", ""),
                git_branch=info.get("git_branch", ""),
                summary=info.get("summary", "")
            ))

        return PaginatedSessions(items=sessions, total=total, page=page, page_size=page_size)

    def _list_codex_sessions(self, project_name: str, page: int, page_size: int) -> PaginatedSessions:
        """List Codex sessions for a project (cwd)."""
        sessions_dir = self._get_codex_sessions_dir()
        if not sessions_dir.exists():
            return PaginatedSessions(items=[], total=0, page=page, page_size=page_size)

        # Find all sessions matching this cwd
        session_files = []
        for session_file in sessions_dir.rglob("rollout-*.jsonl"):
            try:
                cwd = self._extract_codex_cwd(session_file)
                if cwd == project_name:
                    stat = session_file.stat()
                    session_files.append((session_file, stat))
            except OSError:
                continue

        session_files.sort(key=lambda x: x[1].st_mtime, reverse=True)
        total = len(session_files)

        start = (page - 1) * page_size
        end = start + page_size
        page_files = session_files[start:end]

        sessions = []
        for session_file, stat in page_files:
            session_id = session_file.stem
            info = self._parse_codex_session_info(session_file)
            sessions.append(SessionInfo(
                session_id=session_id,
                size=stat.st_size,
                mtime=stat.st_mtime,
                first_message=info.get("first_message", ""),
                git_branch=info.get("git_branch", ""),
                summary=info.get("summary", "")
            ))

        return PaginatedSessions(items=sessions, total=total, page=page, page_size=page_size)

    def _list_gemini_sessions(self, project_name: str, page: int, page_size: int) -> PaginatedSessions:
        """List Gemini sessions for a project (hash directory)."""
        tmp_dir = self._get_gemini_tmp_dir()
        project_dir = tmp_dir / project_name / "chats"

        if not project_dir.exists():
            return PaginatedSessions(items=[], total=0, page=page, page_size=page_size)

        session_files = []
        for session_file in project_dir.glob("session-*.json"):
            try:
                stat = session_file.stat()
                session_files.append((session_file, stat))
            except OSError:
                continue

        session_files.sort(key=lambda x: x[1].st_mtime, reverse=True)
        total = len(session_files)

        start = (page - 1) * page_size
        end = start + page_size
        page_files = session_files[start:end]

        sessions = []
        for session_file, stat in page_files:
            session_id = session_file.stem
            info = self._parse_gemini_session_info(session_file)
            sessions.append(SessionInfo(
                session_id=session_id,
                size=stat.st_size,
                mtime=stat.st_mtime,
                first_message=info.get("first_message", ""),
                git_branch=info.get("git_branch", ""),
                summary=info.get("summary", "")
            ))

        return PaginatedSessions(items=sessions, total=total, page=page, page_size=page_size)

    def _parse_claude_session_info(self, file_path: Path) -> dict:
        """Parse Claude Code session file to extract info."""
        result = {"first_message": "", "git_branch": "", "summary": ""}

        try:
            file_size = file_path.stat().st_size
            if file_size > 10 * 1024 * 1024:
                with open(file_path, "r", encoding="utf-8") as f:
                    head_content = f.read(32 * 1024)
                lines = head_content.split("\n")[:50]
            else:
                with open(file_path, "r", encoding="utf-8") as f:
                    lines = f.readlines()[:50]

            for line in lines:
                line = line.strip()
                if not line:
                    continue
                try:
                    data = json.loads(line)
                    if data.get("type") == "summary" and data.get("summary"):
                        result["summary"] = data["summary"]
                    if data.get("gitBranch") and not result["git_branch"]:
                        result["git_branch"] = data["gitBranch"]
                    if data.get("type") == "user" and data.get("message"):
                        msg = data["message"]
                        content = msg.get("content", "")
                        if content and content != "Warmup" and not result["first_message"]:
                            if isinstance(content, str):
                                result["first_message"] = content[:200]
                            elif isinstance(content, list):
                                for item in content:
                                    if isinstance(item, dict) and item.get("type") == "text":
                                        result["first_message"] = item.get("text", "")[:200]
                                        break
                except json.JSONDecodeError:
                    continue
        except Exception:
            pass

        return result

    def _parse_codex_session_info(self, file_path: Path) -> dict:
        """Parse Codex session file to extract info."""
        result = {"first_message": "", "git_branch": "", "summary": ""}

        try:
            with open(file_path, "r", encoding="utf-8") as f:
                for line in f:
                    line = line.strip()
                    if not line:
                        continue
                    try:
                        data = json.loads(line)
                        msg_type = data.get("type")

                        if msg_type == "event_msg":
                            payload = data.get("payload", {})
                            if payload.get("type") == "user_message" and not result["first_message"]:
                                message = payload.get("message", "")
                                if message and message != "Warmup":
                                    result["first_message"] = message[:200]
                    except json.JSONDecodeError:
                        continue
        except Exception:
            pass

        return result

    def _parse_gemini_session_info(self, file_path: Path) -> dict:
        """Parse Gemini session JSON file for session info."""
        result = {"first_message": "", "git_branch": "", "summary": "", "cwd": ""}

        try:
            with open(file_path, "r", encoding="utf-8") as f:
                data = json.load(f)
                result["cwd"] = data.get("projectPath", "")

                messages = data.get("messages", [])
                for msg in messages:
                    if msg.get("type") == "user" and not result["first_message"]:
                        content = msg.get("content", "")
                        if content:
                            result["first_message"] = content[:200]
                            break
        except Exception:
            pass

        return result

    def get_session_messages(self, cli_type: str, project_name: str, session_id: str) -> list[dict]:
        """Get all messages from a session."""
        if cli_type == "codex":
            return self._get_codex_messages(project_name, session_id)
        elif cli_type == "gemini":
            return self._get_gemini_messages(project_name, session_id)
        else:
            return self._get_claude_messages(project_name, session_id)

    def _get_claude_messages(self, project_name: str, session_id: str) -> list[dict]:
        """Get messages from Claude Code session."""
        projects_dir = self._get_projects_dir("claude_code")
        session_file = projects_dir / project_name / f"{session_id}.jsonl"

        if not session_file.exists():
            return []

        messages = []
        try:
            with open(session_file, "r", encoding="utf-8") as f:
                for line in f:
                    line = line.strip()
                    if not line:
                        continue
                    try:
                        data = json.loads(line)
                        msg_type = data.get("type")
                        if msg_type in ("user", "assistant"):
                            message = data.get("message", {})
                            content = message.get("content", "")

                            if isinstance(content, list):
                                text_parts = []
                                for item in content:
                                    if isinstance(item, dict):
                                        item_type = item.get("type")

                                        if item_type == "text":
                                            text_parts.append(item.get("text", ""))

                                        elif item_type == "tool_result" and msg_type == "user":
                                            result_content = item.get("content", "")
                                            if isinstance(result_content, str):
                                                text_parts.append(f"**[工具结果]**\n```\n{result_content}\n```")
                                            else:
                                                text_parts.append(f"**[工具结果]**\n```json\n{json.dumps(result_content, ensure_ascii=False, indent=2)}\n```")

                                        elif item_type == "tool_use" and msg_type == "assistant":
                                            tool_name = item.get("name", "unknown")
                                            tool_input = item.get("input", {})
                                            input_str = json.dumps(tool_input, ensure_ascii=False, indent=2)
                                            text_parts.append(f"**[调用工具: {tool_name}]**\n```json\n{input_str}\n```")

                                        elif item_type == "thinking" and msg_type == "assistant":
                                            thinking = item.get("thinking", "")
                                            if thinking:
                                                text_parts.append(f"**[思考]**\n{thinking}")

                                        elif item_type == "image":
                                            text_parts.append("[图片]")

                                content = "\n\n".join(text_parts)

                            if content and content != "Warmup":
                                messages.append({
                                    "role": msg_type,
                                    "content": content,
                                    "timestamp": data.get("timestamp")
                                })
                    except json.JSONDecodeError:
                        continue
        except Exception:
            pass

        return messages

    def _get_codex_messages(self, project_name: str, session_id: str) -> list[dict]:
        """Get messages from Codex session."""
        sessions_dir = self._get_codex_sessions_dir()
        session_file = None

        # Find the session file
        for f in sessions_dir.rglob(f"{session_id}.jsonl"):
            cwd = self._extract_codex_cwd(f)
            if cwd == project_name:
                session_file = f
                break

        if not session_file or not session_file.exists():
            return []

        messages = []
        try:
            with open(session_file, "r", encoding="utf-8") as f:
                for line in f:
                    line = line.strip()
                    if not line:
                        continue
                    try:
                        data = json.loads(line)
                        msg_type = data.get("type")
                        timestamp = data.get("timestamp")

                        # Only process response_item for structured messages
                        # Skip event_msg to avoid duplicates (event_msg is for real-time streaming)
                        if msg_type == "response_item":
                            payload = data.get("payload", {})
                            item_type = payload.get("type")
                            role = payload.get("role")

                            # User context messages (AGENTS.md, environment_context, user input)
                            if role == "user" and item_type == "message":
                                content_list = payload.get("content", [])
                                text_parts = []
                                for item in content_list:
                                    if isinstance(item, dict) and item.get("type") == "input_text":
                                        text_parts.append(item.get("text", ""))
                                content = "\n\n".join(text_parts)
                                if content:
                                    messages.append({
                                        "role": "user",
                                        "content": content,
                                        "timestamp": timestamp
                                    })

                            # Assistant text messages
                            elif role == "assistant" and item_type == "message":
                                content_list = payload.get("content", [])
                                text_parts = []
                                for item in content_list:
                                    if isinstance(item, dict):
                                        if item.get("type") in ("output_text", "text"):
                                            text_parts.append(item.get("text", ""))
                                content = "\n\n".join(text_parts)
                                if content:
                                    messages.append({
                                        "role": "assistant",
                                        "content": content,
                                        "timestamp": timestamp
                                    })

                            # Reasoning summary
                            elif item_type == "reasoning":
                                summary = payload.get("summary", [])
                                text_parts = []
                                for item in summary:
                                    if isinstance(item, dict) and item.get("type") == "summary_text":
                                        text_parts.append(item.get("text", ""))
                                content = "\n".join(text_parts)
                                if content:
                                    messages.append({
                                        "role": "assistant",
                                        "content": f"**[推理]**\n{content}",
                                        "timestamp": timestamp
                                    })

                            # Function call (tool use)
                            elif item_type == "function_call":
                                name = payload.get("name", "unknown")
                                arguments = payload.get("arguments", "{}")
                                try:
                                    args_obj = json.loads(arguments)
                                    args_str = json.dumps(args_obj, ensure_ascii=False, indent=2)
                                except:
                                    args_str = arguments
                                messages.append({
                                    "role": "assistant",
                                    "content": f"**[调用工具: {name}]**\n```json\n{args_str}\n```",
                                    "timestamp": timestamp
                                })

                            # Function call output (tool result)
                            elif item_type == "function_call_output":
                                output = payload.get("output", "")
                                messages.append({
                                    "role": "user",
                                    "content": f"**[工具结果]**\n```\n{output}\n```",
                                    "timestamp": timestamp
                                })

                    except json.JSONDecodeError:
                        continue
        except Exception:
            pass

        return messages

    def _get_gemini_messages(self, project_name: str, session_id: str) -> list[dict]:
        """Get messages from Gemini session."""
        tmp_dir = self._get_gemini_tmp_dir()
        session_file = tmp_dir / project_name / "chats" / f"{session_id}.json"

        if not session_file.exists():
            return []

        messages = []
        try:
            with open(session_file, "r", encoding="utf-8") as f:
                data = json.load(f)
                for msg in data.get("messages", []):
                    msg_type = msg.get("type")
                    content = msg.get("content", "")

                    if msg_type == "user":
                        if content:
                            messages.append({
                                "role": "user",
                                "content": content,
                                "timestamp": msg.get("timestamp")
                            })
                    elif msg_type == "gemini":
                        text_parts = []
                        if content:
                            text_parts.append(content)

                        # Handle thoughts
                        for thought in msg.get("thoughts", []):
                            desc = thought.get("description", "")
                            if desc:
                                text_parts.append(f"**[思考]**\n{desc}")

                        # Handle tool calls
                        for tool_call in msg.get("toolCalls", []):
                            tool_name = tool_call.get("displayName") or tool_call.get("name", "unknown")
                            result_display = tool_call.get("resultDisplay", "")
                            if result_display:
                                text_parts.append(f"**[工具: {tool_name}]**\n{result_display}")

                        final_content = "\n\n".join(text_parts)
                        if final_content:
                            messages.append({
                                "role": "assistant",
                                "content": final_content,
                                "timestamp": msg.get("timestamp")
                            })
        except Exception:
            pass

        return messages

    def delete_session(self, cli_type: str, project_name: str, session_id: str) -> bool:
        """Delete a session file."""
        if cli_type == "codex":
            return self._delete_codex_session(project_name, session_id)
        elif cli_type == "gemini":
            return self._delete_gemini_session(project_name, session_id)
        else:
            return self._delete_claude_session(project_name, session_id)

    def _delete_claude_session(self, project_name: str, session_id: str) -> bool:
        projects_dir = self._get_projects_dir("claude_code")
        session_file = projects_dir / project_name / f"{session_id}.jsonl"
        if session_file.exists():
            session_file.unlink()
            return True
        return False

    def _delete_codex_session(self, project_name: str, session_id: str) -> bool:
        sessions_dir = self._get_codex_sessions_dir()
        for f in sessions_dir.rglob(f"{session_id}.jsonl"):
            cwd = self._extract_codex_cwd(f)
            if cwd == project_name:
                f.unlink()
                return True
        return False

    def _delete_gemini_session(self, project_name: str, session_id: str) -> bool:
        tmp_dir = self._get_gemini_tmp_dir()
        session_file = tmp_dir / project_name / "chats" / f"{session_id}.json"
        if session_file.exists():
            session_file.unlink()
            return True
        return False

    def delete_project(self, cli_type: str, project_name: str) -> bool:
        """Delete a project directory."""
        if cli_type == "codex":
            return self._delete_codex_project(project_name)
        elif cli_type == "gemini":
            return self._delete_gemini_project(project_name)
        else:
            return self._delete_claude_project(project_name)

    def _delete_claude_project(self, project_name: str) -> bool:
        projects_dir = self._get_projects_dir("claude_code")
        project_dir = projects_dir / project_name
        if project_dir.exists():
            shutil.rmtree(project_dir)
            return True
        return False

    def _delete_codex_project(self, project_name: str) -> bool:
        """Delete all Codex sessions for a project (cwd)."""
        sessions_dir = self._get_codex_sessions_dir()
        deleted = False
        for f in sessions_dir.rglob("rollout-*.jsonl"):
            cwd = self._extract_codex_cwd(f)
            if cwd == project_name:
                f.unlink()
                deleted = True
        return deleted

    def _delete_gemini_project(self, project_name: str) -> bool:
        tmp_dir = self._get_gemini_tmp_dir()
        project_dir = tmp_dir / project_name
        if project_dir.exists():
            shutil.rmtree(project_dir)
            return True
        return False
