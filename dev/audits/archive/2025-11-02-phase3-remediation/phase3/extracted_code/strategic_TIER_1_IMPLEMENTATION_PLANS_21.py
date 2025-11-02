# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 21
# src/fraiseql/enterprise/audit/compliance_reports.py

from datetime import datetime
from typing import Any

from fraiseql.enterprise.audit.verification import verify_chain

from fraiseql.db import DatabaseQuery, FraiseQLRepository


async def generate_sox_report(
    repo: FraiseQLRepository,
    start_date: datetime,
    end_date: datetime,
    tenant_id: Optional[str] = None,
) -> dict[str, Any]:
    """Generate SOX compliance report.

    SOX requirements:
    - Immutable audit trail
    - Access controls
    - Segregation of duties
    - Change tracking

    Args:
        repo: Database repository
        start_date: Report period start
        end_date: Report period end
        tenant_id: Optional tenant filter

    Returns:
        SOX compliance report
    """
    # Verify chain integrity
    chain_result = await verify_chain(repo, tenant_id)

    # Get event counts by type
    events = await repo.run(
        DatabaseQuery(
            statement="""
            SELECT event_type, COUNT(*) as count
            FROM audit_events
            WHERE timestamp >= %s AND timestamp <= %s
            AND (tenant_id = %s OR (tenant_id IS NULL AND %s IS NULL))
            GROUP BY event_type
        """,
            params={"start_date": start_date, "end_date": end_date, "tenant_id": tenant_id},
            fetch_result=True,
        )
    )

    # Analyze segregation of duties
    # (e.g., same user shouldn't create and approve financial transactions)
    violations = await _check_segregation_violations(repo, start_date, end_date)

    return {
        "period": {"start": start_date.isoformat(), "end": end_date.isoformat()},
        "chain_integrity": chain_result,
        "total_events": chain_result["total_events"],
        "events_by_type": {e["event_type"]: e["count"] for e in events},
        "segregation_of_duties": {"violations": len(violations), "details": violations},
        "compliant": chain_result["valid"] and len(violations) == 0,
    }


async def _check_segregation_violations(
    repo: FraiseQLRepository, start_date: datetime, end_date: datetime
) -> list[dict]:
    """Check for segregation of duties violations."""
    # Find cases where same user created and approved
    results = await repo.run(
        DatabaseQuery(
            statement="""
            WITH transactions AS (
                SELECT
                    event_data->>'transaction_id' as tx_id,
                    user_id
                FROM audit_events
                WHERE event_type = 'financial.transaction'
                AND timestamp >= %s AND timestamp <= %s
            ),
            approvals AS (
                SELECT
                    event_data->>'transaction_id' as tx_id,
                    user_id
                FROM audit_events
                WHERE event_type = 'financial.approval'
                AND timestamp >= %s AND timestamp <= %s
            )
            SELECT t.tx_id, t.user_id
            FROM transactions t
            INNER JOIN approvals a ON t.tx_id = a.tx_id
            WHERE t.user_id = a.user_id
        """,
            params={"start_date": start_date, "end_date": end_date},
            fetch_result=True,
        )
    )

    return [
        {
            "transaction_id": r["tx_id"],
            "user_id": str(r["user_id"]),
            "violation": "same_user_create_and_approve",
        }
        for r in results
    ]
