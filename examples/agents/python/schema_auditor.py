#!/usr/bin/env python3
"""
FraiseQL Schema Auditor - Design Quality Agent

Analyzes compiled GraphQL schemas for design quality issues using the
fraiseql-server design audit APIs. Produces detailed HTML reports with
issue prioritization, recommendations, and federation visualization.

Usage:
    python schema_auditor.py schema.compiled.json
    python schema_auditor.py schema.compiled.json --api-endpoint http://localhost:8080
    python schema_auditor.py schema.compiled.json --output report.html
    python schema_auditor.py schema.compiled.json --format json
"""

import json
import sys
import argparse
from pathlib import Path
from typing import Dict, Any, Optional, List
from datetime import datetime
from dataclasses import dataclass
import requests
from urllib.parse import urljoin


@dataclass
class DesignScore:
    """Design quality scores"""
    overall: int
    federation: int
    cost: int
    cache: int
    authorization: int
    compilation: int

    def to_dict(self) -> Dict[str, int]:
        """Convert to dictionary"""
        return {
            'overall': self.overall,
            'federation': self.federation,
            'cost': self.cost,
            'cache': self.cache,
            'authorization': self.authorization,
            'compilation': self.compilation,
        }


@dataclass
class SeverityCounts:
    """Issue severity counts"""
    critical: int
    warning: int
    info: int


@dataclass
class DesignIssue:
    """Single design issue"""
    category: str
    severity: str
    message: str
    suggestion: str
    affected: Optional[str] = None


class SchemaAuditor:
    """Analyzes schema design quality using fraiseql-server APIs"""

    def __init__(
        self,
        schema_path: str,
        api_endpoint: str = 'http://localhost:8080',
        timeout: int = 30,
    ):
        """
        Initialize schema auditor

        Args:
            schema_path: Path to compiled schema JSON file
            api_endpoint: fraiseql-server base URL
            timeout: Request timeout in seconds
        """
        self.schema_path = Path(schema_path)
        self.api_endpoint = api_endpoint.rstrip('/')
        self.timeout = timeout
        self.schema: Optional[Dict[str, Any]] = None
        self.audit_response: Optional[Dict[str, Any]] = None
        self.issues: List[DesignIssue] = []
        self.scores: Optional[DesignScore] = None
        self.severity_counts: Optional[SeverityCounts] = None

    def load_schema(self) -> bool:
        """Load and validate schema file"""
        if not self.schema_path.exists():
            print(f"Error: Schema file not found: {self.schema_path}")
            return False

        try:
            with open(self.schema_path) as f:
                self.schema = json.load(f)
            return True
        except json.JSONDecodeError as e:
            print(f"Error: Invalid JSON in schema file: {e}")
            return False
        except Exception as e:
            print(f"Error reading schema file: {e}")
            return False

    def analyze(self) -> bool:
        """Run design audit against fraiseql-server"""
        if not self.schema:
            print("Error: Schema not loaded")
            return False

        try:
            # Call design audit API
            url = urljoin(self.api_endpoint, '/api/v1/design/audit')
            print(f"Calling design audit API: {url}")

            response = requests.post(
                url,
                json={'schema': self.schema},
                timeout=self.timeout,
            )

            if response.status_code != 200:
                print(f"Error: API returned {response.status_code}")
                print(response.text)
                return False

            self.audit_response = response.json()

            # Extract scores and issues
            if not self._parse_audit_response():
                return False

            return True

        except requests.ConnectionError:
            print(f"Error: Could not connect to {self.api_endpoint}")
            print("Make sure fraiseql-server is running and accessible")
            return False
        except requests.Timeout:
            print(f"Error: Request timed out after {self.timeout}s")
            return False
        except Exception as e:
            print(f"Error calling design API: {e}")
            return False

    def _parse_audit_response(self) -> bool:
        """Parse audit response and extract scores/issues"""
        if not self.audit_response:
            return False

        try:
            data = self.audit_response.get('data', {})

            # Extract scores
            self.scores = DesignScore(
                overall=data.get('overall_score', 0),
                federation=data.get('federation', {}).get('score', 0),
                cost=data.get('cost', {}).get('score', 0),
                cache=data.get('cache', {}).get('score', 0),
                authorization=data.get('authorization', {}).get('score', 0),
                compilation=data.get('compilation', {}).get('score', 0),
            )

            # Extract severity counts
            severity_data = data.get('severity_counts', {})
            self.severity_counts = SeverityCounts(
                critical=severity_data.get('critical', 0),
                warning=severity_data.get('warning', 0),
                info=severity_data.get('info', 0),
            )

            # Extract issues from each category
            categories = ['federation', 'cost', 'cache', 'authorization', 'compilation']
            for category in categories:
                category_data = data.get(category, {})
                for issue_data in category_data.get('issues', []):
                    issue = DesignIssue(
                        category=category,
                        severity=issue_data.get('severity', 'info'),
                        message=issue_data.get('message', ''),
                        suggestion=issue_data.get('suggestion', ''),
                        affected=issue_data.get('affected'),
                    )
                    self.issues.append(issue)

            return True

        except Exception as e:
            print(f"Error parsing audit response: {e}")
            return False

    def get_score_interpretation(self) -> str:
        """Get human-readable interpretation of overall score"""
        if not self.scores:
            return "Unknown"

        score = self.scores.overall
        if score >= 90:
            return "Excellent"
        elif score >= 75:
            return "Good"
        elif score >= 60:
            return "Fair"
        else:
            return "Poor"

    def get_priority_issues(self, limit: int = 5) -> List[DesignIssue]:
        """Get top priority issues (critical first)"""
        severity_order = {'critical': 0, 'warning': 1, 'info': 2}

        sorted_issues = sorted(
            self.issues,
            key=lambda x: severity_order.get(x.severity, 3),
        )

        return sorted_issues[:limit]

    def generate_html_report(self) -> str:
        """Generate HTML report"""
        if not self.scores or not self.severity_counts:
            return "<html><body><p>Error: No data to report</p></body></html>"

        score = self.scores.overall
        interpretation = self.get_score_interpretation()
        priority_issues = self.get_priority_issues()

        # Determine score color
        if score >= 90:
            score_color = '#4CAF50'  # Green
        elif score >= 75:
            score_color = '#2196F3'  # Blue
        elif score >= 60:
            score_color = '#FF9800'  # Orange
        else:
            score_color = '#F44336'  # Red

        # Build issue list HTML
        issues_html = ""
        for issue in priority_issues:
            severity_color = {
                'critical': '#F44336',
                'warning': '#FF9800',
                'info': '#2196F3',
            }.get(issue.severity, '#999')

            issues_html += f"""
            <div class="issue" style="border-left: 4px solid {severity_color};">
                <div class="issue-header">
                    <span class="severity" style="background-color: {severity_color};">
                        {issue.severity.upper()}
                    </span>
                    <span class="category">{issue.category.upper()}</span>
                    {f'<span class="affected">{issue.affected}</span>' if issue.affected else ''}
                </div>
                <p class="message">{issue.message}</p>
                <p class="suggestion"><strong>Suggestion:</strong> {issue.suggestion}</p>
            </div>
            """

        html = f"""<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>FraiseQL Design Audit Report</title>
    <style>
        * {{
            margin: 0;
            padding: 0;
            box-sizing: border-box;
        }}
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
            background: #f5f5f5;
            padding: 20px;
        }}
        .container {{
            max-width: 1200px;
            margin: 0 auto;
            background: white;
            border-radius: 8px;
            box-shadow: 0 2px 8px rgba(0,0,0,0.1);
            overflow: hidden;
        }}
        .header {{
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            color: white;
            padding: 40px;
            text-align: center;
        }}
        .header h1 {{
            margin-bottom: 10px;
        }}
        .header p {{
            opacity: 0.9;
            font-size: 14px;
        }}
        .score-display {{
            display: flex;
            justify-content: center;
            align-items: center;
            gap: 40px;
            margin: 40px 0;
            flex-wrap: wrap;
        }}
        .score-circle {{
            width: 140px;
            height: 140px;
            border-radius: 50%;
            display: flex;
            flex-direction: column;
            justify-content: center;
            align-items: center;
            background-color: {score_color};
            color: white;
            font-size: 48px;
            font-weight: bold;
            box-shadow: 0 4px 12px rgba(0,0,0,0.15);
        }}
        .score-circle .label {{
            font-size: 12px;
            font-weight: normal;
            margin-top: 8px;
        }}
        .interpretation {{
            font-size: 24px;
            font-weight: 600;
            text-align: center;
            color: {score_color};
        }}
        .content {{
            padding: 40px;
        }}
        .section {{
            margin-bottom: 40px;
        }}
        .section h2 {{
            font-size: 20px;
            color: #333;
            margin-bottom: 20px;
            border-bottom: 2px solid #667eea;
            padding-bottom: 10px;
        }}
        .categories {{
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
            gap: 20px;
            margin-bottom: 30px;
        }}
        .category-card {{
            background: #f9f9f9;
            border-radius: 8px;
            padding: 20px;
            text-align: center;
            border-top: 3px solid #667eea;
        }}
        .category-name {{
            font-size: 12px;
            font-weight: 600;
            text-transform: uppercase;
            color: #666;
            margin-bottom: 10px;
        }}
        .category-score {{
            font-size: 32px;
            font-weight: bold;
            color: #667eea;
        }}
        .category-score.excellent {{ color: #4CAF50; }}
        .category-score.good {{ color: #2196F3; }}
        .category-score.fair {{ color: #FF9800; }}
        .category-score.poor {{ color: #F44336; }}
        .severity-counts {{
            display: flex;
            gap: 20px;
            margin: 20px 0;
        }}
        .count {{
            display: flex;
            align-items: center;
            gap: 10px;
        }}
        .count-badge {{
            width: 40px;
            height: 40px;
            border-radius: 50%;
            display: flex;
            justify-content: center;
            align-items: center;
            color: white;
            font-weight: bold;
        }}
        .count-badge.critical {{ background-color: #F44336; }}
        .count-badge.warning {{ background-color: #FF9800; }}
        .count-badge.info {{ background-color: #2196F3; }}
        .issue {{
            background: #f9f9f9;
            border-radius: 6px;
            padding: 16px;
            margin-bottom: 12px;
        }}
        .issue-header {{
            display: flex;
            gap: 10px;
            margin-bottom: 10px;
            flex-wrap: wrap;
            align-items: center;
        }}
        .severity, .category, .affected {{
            font-size: 11px;
            font-weight: 600;
            text-transform: uppercase;
            color: white;
            padding: 4px 12px;
            border-radius: 4px;
        }}
        .category {{
            background-color: #667eea;
        }}
        .affected {{
            background-color: #999;
        }}
        .message {{
            color: #333;
            margin-bottom: 8px;
            line-height: 1.5;
        }}
        .suggestion {{
            color: #666;
            font-size: 13px;
            line-height: 1.5;
        }}
        .footer {{
            background: #f5f5f5;
            border-top: 1px solid #eee;
            padding: 20px 40px;
            font-size: 12px;
            color: #666;
            display: flex;
            justify-content: space-between;
            align-items: center;
        }}
        @media (max-width: 600px) {{
            .container {{
                border-radius: 0;
            }}
            .header {{
                padding: 20px;
            }}
            .score-display {{
                gap: 20px;
            }}
            .content {{
                padding: 20px;
            }}
            .footer {{
                padding: 15px 20px;
                flex-direction: column;
                gap: 10px;
                text-align: center;
            }}
        }}
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>FraiseQL Design Audit Report</h1>
            <p>{self.schema_path.name} · {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}</p>
        </div>

        <div class="content">
            <div class="score-display">
                <div class="score-circle">
                    {self.scores.overall}
                    <div class="label">Overall</div>
                </div>
                <div class="interpretation">{interpretation}</div>
            </div>

            <div class="section">
                <h2>Category Scores</h2>
                <div class="categories">
                    <div class="category-card">
                        <div class="category-name">Federation</div>
                        <div class="category-score">{self.scores.federation}</div>
                    </div>
                    <div class="category-card">
                        <div class="category-name">Cost</div>
                        <div class="category-score">{self.scores.cost}</div>
                    </div>
                    <div class="category-card">
                        <div class="category-name">Cache</div>
                        <div class="category-score">{self.scores.cache}</div>
                    </div>
                    <div class="category-card">
                        <div class="category-name">Authorization</div>
                        <div class="category-score">{self.scores.authorization}</div>
                    </div>
                    <div class="category-card">
                        <div class="category-name">Compilation</div>
                        <div class="category-score">{self.scores.compilation}</div>
                    </div>
                </div>
            </div>

            <div class="section">
                <h2>Issue Summary</h2>
                <div class="severity-counts">
                    <div class="count">
                        <div class="count-badge critical">{self.severity_counts.critical}</div>
                        <div>Critical</div>
                    </div>
                    <div class="count">
                        <div class="count-badge warning">{self.severity_counts.warning}</div>
                        <div>Warnings</div>
                    </div>
                    <div class="count">
                        <div class="count-badge info">{self.severity_counts.info}</div>
                        <div>Info</div>
                    </div>
                </div>
            </div>

            {f'''<div class="section">
                <h2>Top Issues</h2>
                {issues_html if issues_html else '<p>No issues detected. Design is excellent!</p>'}
            </div>''' if self.issues else '<div class="section"><p style="color: #4CAF50; font-weight: 500;">✓ No design issues detected!</p></div>'}
        </div>

        <div class="footer">
            <div>Generated by FraiseQL Schema Auditor</div>
            <div><a href="https://fraiseql.dev/docs" style="color: #667eea;">View Documentation</a></div>
        </div>
    </div>
</body>
</html>
"""
        return html

    def save_report(self, output_path: str, format_type: str = 'html') -> bool:
        """Save report to file"""
        try:
            output_file = Path(output_path)
            output_file.parent.mkdir(parents=True, exist_ok=True)

            if format_type == 'html':
                content = self.generate_html_report()
                output_file.write_text(content)
            elif format_type == 'json':
                data = {
                    'timestamp': datetime.now().isoformat(),
                    'schema_file': str(self.schema_path),
                    'scores': self.scores.to_dict() if self.scores else {},
                    'severity_counts': {
                        'critical': self.severity_counts.critical,
                        'warning': self.severity_counts.warning,
                        'info': self.severity_counts.info,
                    } if self.severity_counts else {},
                    'audit_response': self.audit_response,
                }
                output_file.write_text(json.dumps(data, indent=2))
            else:
                print(f"Error: Unknown format type: {format_type}")
                return False

            print(f"Report saved to: {output_file.absolute()}")
            return True

        except Exception as e:
            print(f"Error saving report: {e}")
            return False


def main():
    """Main CLI entry point"""
    parser = argparse.ArgumentParser(
        description='FraiseQL Schema Auditor - Design Quality Analysis',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  python schema_auditor.py schema.compiled.json
  python schema_auditor.py schema.compiled.json --api-endpoint http://api.local:8080
  python schema_auditor.py schema.compiled.json --output audit-report.html
  python schema_auditor.py schema.compiled.json --format json
        """,
    )

    parser.add_argument('schema', help='Path to compiled schema JSON file')
    parser.add_argument(
        '--api-endpoint',
        default='http://localhost:8080',
        help='fraiseql-server API endpoint (default: http://localhost:8080)',
    )
    parser.add_argument(
        '--output',
        help='Output file path (default: schema-audit-TIMESTAMP.html)',
    )
    parser.add_argument(
        '--format',
        choices=['html', 'json', 'markdown'],
        default='html',
        help='Output format (default: html)',
    )
    parser.add_argument(
        '--fail-if-below',
        type=int,
        help='Exit with code 1 if score is below threshold',
    )
    parser.add_argument(
        '--quiet',
        action='store_true',
        help='Suppress progress output',
    )

    args = parser.parse_args()

    # Validate format vs output extension
    if args.output:
        ext = Path(args.output).suffix.lower()
        if ext in ['.html', '.json', '.md', '.markdown']:
            format_from_ext = {'.html': 'html', '.json': 'json', '.md': 'markdown', '.markdown': 'markdown'}
            args.format = format_from_ext.get(ext, args.format)

    # Create auditor and run analysis
    auditor = SchemaAuditor(args.schema, api_endpoint=args.api_endpoint)

    if not args.quiet:
        print("Loading schema...")
    if not auditor.load_schema():
        return 1

    if not args.quiet:
        print("Running design audit...")
    if not auditor.analyze():
        return 1

    if not args.quiet:
        print(f"Design Score: {auditor.scores.overall}/100 ({auditor.get_score_interpretation()})")

    # Save report
    output_file = args.output or f"schema-audit-{datetime.now().strftime('%Y%m%d-%H%M%S')}.{args.format}"
    if not auditor.save_report(output_file, args.format):
        return 1

    # Check fail threshold
    if args.fail_if_below and auditor.scores.overall < args.fail_if_below:
        if not args.quiet:
            print(f"Error: Design score {auditor.scores.overall} is below threshold {args.fail_if_below}")
        return 1

    return 0


if __name__ == '__main__':
    sys.exit(main())
