from sqlalchemy import Column, Integer, String, Text, UniqueConstraint
from app.core.database import LogBase


class RequestLog(LogBase):
    __tablename__ = "request_logs"

    id = Column(Integer, primary_key=True, autoincrement=True)
    created_at = Column(Integer, nullable=False)
    cli_type = Column(String(20), nullable=False)
    provider_name = Column(String(100), nullable=False)
    model_id = Column(String(50), nullable=True)
    success = Column(Integer, nullable=False, default=1)
    status_code = Column(Integer, nullable=True)
    elapsed_ms = Column(Integer, nullable=False, default=0)
    input_tokens = Column(Integer, nullable=False, default=0)
    output_tokens = Column(Integer, nullable=False, default=0)
    # CLI request
    client_method = Column(String(10), nullable=False)
    client_path = Column(String(500), nullable=False)
    client_headers = Column(Text, nullable=False)
    client_body = Column(Text, nullable=False)
    # Gateway forward request
    forward_url = Column(String(1000), nullable=False)
    forward_headers = Column(Text, nullable=False)
    forward_body = Column(Text, nullable=False)
    # Provider response
    provider_status = Column(Integer, nullable=True)
    provider_headers = Column(Text, nullable=True)
    provider_body = Column(Text, nullable=True)
    # Gateway forward response
    response_status = Column(Integer, nullable=True)
    response_headers = Column(Text, nullable=True)
    response_body = Column(Text, nullable=True)
    error_message = Column(Text, nullable=True)


class SystemLog(LogBase):
    __tablename__ = "system_logs"

    id = Column(Integer, primary_key=True, autoincrement=True)
    created_at = Column(Integer, nullable=False)
    level = Column(String(10), nullable=False)  # INFO, WARN, ERROR
    event_type = Column(String(50), nullable=False)
    provider_name = Column(String(100), nullable=True)
    message = Column(Text, nullable=False)
    details = Column(Text, nullable=True)


class UsageDaily(LogBase):
    __tablename__ = "usage_daily"

    id = Column(Integer, primary_key=True, autoincrement=True)
    usage_date = Column(String(10), nullable=False)  # YYYY-MM-DD
    provider_name = Column(String(100), nullable=False)
    cli_type = Column(String(20), nullable=False)
    request_count = Column(Integer, nullable=False, default=0)
    success_count = Column(Integer, nullable=False, default=0)
    failure_count = Column(Integer, nullable=False, default=0)
    prompt_tokens = Column(Integer, nullable=False, default=0)
    completion_tokens = Column(Integer, nullable=False, default=0)

    __table_args__ = (
        UniqueConstraint("usage_date", "provider_name", "cli_type", name="uq_usage_daily"),
    )
