"""Shared test fixtures and configuration."""

import sqlite3
import tempfile
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

from fraisier.config import FraisierConfig
from fraisier.database import FraisierDB, init_database


@pytest.fixture
def tmp_db_path(tmp_path: Path) -> Path:
    """Create temporary database file."""
    db_path = tmp_path / "test.db"
    return db_path


@pytest.fixture
def test_db(tmp_db_path: Path) -> FraisierDB:
    """Create test database with trinity schema.

    Initializes empty database with trinity pattern tables:
    - tb_fraise_state (pk_fraise_state, id UUID, identifier business key)
    - tb_deployment (pk_deployment, id UUID, identifier, fk_fraise_state)
    - tb_webhook_event (pk_webhook_event, id UUID, fk_deployment)
    """
    # Patch get_db_path to use test database
    with patch("fraisier.database.get_db_path", return_value=tmp_db_path):
        db = FraisierDB()
        yield db


@pytest.fixture
def sample_config(tmp_path: Path) -> FraisierConfig:
    """Create sample fraises.yaml configuration."""
    config_file = tmp_path / "fraises.yaml"
    config_file.write_text(
        """
git:
  provider: github
  github:
    webhook_secret: test-secret

fraises:
  my_api:
    type: api
    description: Test API service
    environments:
      development:
        app_path: /tmp/test-api-dev
        systemd_service: test-api-dev.service
        health_check:
          url: http://localhost:8000/health
          timeout: 10
      production:
        app_path: /tmp/test-api-prod
        systemd_service: test-api-prod.service
        git_repo: https://github.com/test/api.git
        health_check:
          url: https://api.example.com/health
          timeout: 30
        database:
          tool: alembic
          strategy: apply

  data_pipeline:
    type: etl
    description: Data ETL pipeline
    environments:
      production:
        app_path: /var/etl
        script_path: scripts/pipeline.py
        database:
          tool: alembic
          strategy: apply

  backup_job:
    type: scheduled
    description: Hourly backup
    environments:
      production:
        systemd_service: backup.service
        systemd_timer: backup.timer
        script_path: /usr/local/bin/backup.sh
"""
    )
    return FraisierConfig(str(config_file))


@pytest.fixture
def mock_subprocess():
    """Mock subprocess.run for testing."""
    with patch("subprocess.run") as mock:
        mock.return_value = MagicMock(
            returncode=0,
            stdout="test output\n",
            stderr="",
        )
        yield mock


@pytest.fixture
def mock_requests():
    """Mock requests.get for health checks."""
    with patch("requests.get") as mock:
        mock.return_value = MagicMock(status_code=200)
        yield mock
