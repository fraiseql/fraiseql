"""FraiseQL CLI commands."""

from fraiseql.cli.commands import sbom
from fraiseql.cli.commands.check import check
from fraiseql.cli.commands.dev import dev
from fraiseql.cli.commands.doctor import doctor
from fraiseql.cli.commands.generate import generate
from fraiseql.cli.commands.init import init as init_command
from fraiseql.cli.commands.migrate import migrate
from fraiseql.cli.commands.sql import sql
from fraiseql.cli.commands.turbo import turbo

__all__ = ["check", "dev", "doctor", "generate", "init_command", "migrate", "sbom", "sql", "turbo"]
