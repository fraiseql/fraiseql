/**
 * FraiseQL Federation Analyzer - Design Quality CI/CD Agent
 *
 * Integrates design quality checks into GitHub workflows:
 * - Monitors schema changes in pull requests
 * - Enforces design quality rules with configurable thresholds
 * - Posts detailed comments with issues and recommendations
 * - Creates GitHub status checks to block merges if needed
 * - Tracks design score trends over time
 *
 * Usage:
 *   npx federation-analyzer --schema schema.compiled.json
 *   npx federation-analyzer --repo owner/repo --pr 123
 */

import { Octokit } from '@octokit/rest';
import axios, { AxiosInstance } from 'axios';
import * as fs from 'fs/promises';
import * as path from 'path';


interface DesignScores {
  overall: number;
  federation: number;
  cost: number;
  cache: number;
  authorization: number;
  compilation: number;
}


interface SeverityCounts {
  critical: number;
  warning: number;
  info: number;
}


interface DesignIssue {
  category: string;
  severity: string;
  message: string;
  suggestion: string;
  affected?: string;
}


interface DesignAuditResponse {
  status: string;
  data: {
    overall_score: number;
    severity_counts: SeverityCounts;
    federation: {
      score: number;
      issues: DesignIssue[];
    };
    cost: {
      score: number;
      issues: DesignIssue[];
    };
    cache: {
      score: number;
      issues: DesignIssue[];
    };
    authorization: {
      score: number;
      issues: DesignIssue[];
    };
    compilation: {
      score: number;
      issues: DesignIssue[];
    };
  };
}


interface FederationAnalyzerConfig {
  apiEndpoint: string;
  criticalThreshold: number;
  warningThreshold: number;
  githubToken: string;
  repo?: string;
  pr?: number;
  failIfBelow?: number;
  quiet?: boolean;
}


class FederationAnalyzer {
  private config: FederationAnalyzerConfig;
  private api: AxiosInstance;
  private github: Octokit;
  private schema: Record<string, any> | null = null;
  private auditResponse: DesignAuditResponse | null = null;

  constructor(config: FederationAnalyzerConfig) {
    this.config = config;
    this.api = axios.create({
      baseURL: config.apiEndpoint,
      timeout: 30000,
    });
    this.github = new Octokit({
      auth: config.githubToken,
    });
  }

  /**
   * Load schema from file
   */
  async loadSchema(schemaPath: string): Promise<boolean> {
    try {
      const content = await fs.readFile(schemaPath, 'utf-8');
      this.schema = JSON.parse(content);
      if (!this.config.quiet) {
        console.log(`Schema loaded: ${schemaPath}`);
      }
      return true;
    } catch (error) {
      console.error(`Error loading schema: ${error}`);
      return false;
    }
  }

  /**
   * Run design audit
   */
  async analyze(): Promise<boolean> {
    if (!this.schema) {
      console.error('Error: Schema not loaded');
      return false;
    }

    try {
      if (!this.config.quiet) {
        console.log('Running design audit...');
      }

      const response = await this.api.post<DesignAuditResponse>(
        '/api/v1/design/audit',
        { schema: this.schema },
      );

      this.auditResponse = response.data;

      if (!this.config.quiet) {
        console.log(`Design Score: ${this.auditResponse.data.overall_score}/100`);
      }

      return true;
    } catch (error) {
      console.error(`Error calling design API: ${error}`);
      return false;
    }
  }

  /**
   * Generate GitHub PR comment
   */
  generatePRComment(): string {
    if (!this.auditResponse) {
      return '❌ Design audit failed';
    }

    const data = this.auditResponse.data;
    const score = data.overall_score;
    const severity = data.severity_counts;

    // Determine verdict
    let verdict = '✅ Design review passed';
    let emoji = '✅';

    if (score < this.config.criticalThreshold) {
      verdict = '🚫 Critical design issues detected - merge blocked';
      emoji = '🚫';
    } else if (score < this.config.warningThreshold) {
      verdict = '⚠️ Design review suggested improvements';
      emoji = '⚠️';
    }

    // Format category breakdown
    const categoryBreakdown = `
| Category | Score |
|----------|-------|
| Federation | ${data.federation.score} |
| Cost | ${data.cost.score} |
| Cache | ${data.cache.score} |
| Authorization | ${data.authorization.score} |
| Compilation | ${data.compilation.score} |
`;

    // Format issues
    let issuesMarkdown = '';
    const allIssues = this.getTopIssues(5);

    if (allIssues.length > 0) {
      issuesMarkdown = '\n\n### Issues Found\n\n';
      for (const issue of allIssues) {
        const severityEmoji = {
          critical: '🔴',
          warning: '🟡',
          info: '🔵',
        }[issue.severity] || '⚪';

        issuesMarkdown += `${severityEmoji} **${issue.category}**: ${issue.message}\n`;
        issuesMarkdown += `   → ${issue.suggestion}\n\n`;
      }
    }

    return `## ${emoji} Design Quality Review

**Overall Score:** \`${score}/100\`

### Category Breakdown
${categoryBreakdown}

### Issues Summary
- Critical: ${severity.critical}
- Warnings: ${severity.warning}
- Info: ${severity.info}

### Verdict
${verdict}
${issuesMarkdown}

---
*Design quality check by [FraiseQL Federation Analyzer](https://fraiseql.dev)*`;
  }

  /**
   * Get top priority issues
   */
  private getTopIssues(limit: number): DesignIssue[] {
    if (!this.auditResponse) {
      return [];
    }

    const allIssues: DesignIssue[] = [];
    const data = this.auditResponse.data;

    // Collect all issues
    ['federation', 'cost', 'cache', 'authorization', 'compilation'].forEach(category => {
      const categoryData = data[category as keyof typeof data] as any;
      allIssues.push(...(categoryData.issues || []));
    });

    // Sort by severity
    const severityOrder = { critical: 0, warning: 1, info: 2 };
    allIssues.sort((a, b) => {
      const aOrder = severityOrder[a.severity as keyof typeof severityOrder] || 3;
      const bOrder = severityOrder[b.severity as keyof typeof severityOrder] || 3;
      return aOrder - bOrder;
    });

    return allIssues.slice(0, limit);
  }

  /**
   * Post comment to GitHub PR
   */
  async postPRComment(): Promise<boolean> {
    if (!this.config.repo || !this.config.pr) {
      console.error('Error: repo and pr parameters required');
      return false;
    }

    try {
      const [owner, repo] = this.config.repo.split('/');
      const comment = this.generatePRComment();

      await this.github.issues.createComment({
        owner,
        repo,
        issue_number: this.config.pr,
        body: comment,
      });

      if (!this.config.quiet) {
        console.log(`Comment posted to PR #${this.config.pr}`);
      }

      return true;
    } catch (error) {
      console.error(`Error posting comment: ${error}`);
      return false;
    }
  }

  /**
   * Create or update GitHub status check
   */
  async createStatusCheck(commitSha: string): Promise<boolean> {
    if (!this.config.repo) {
      console.error('Error: repo parameter required');
      return false;
    }

    if (!this.auditResponse) {
      console.error('Error: Design audit not run');
      return false;
    }

    try {
      const [owner, repo] = this.config.repo.split('/');
      const score = this.auditResponse.data.overall_score;

      let state: 'success' | 'failure' | 'pending' = 'success';
      let description = `Design score: ${score}/100 - Excellent`;

      if (score < this.config.criticalThreshold) {
        state = 'failure';
        description = `Design score: ${score}/100 - Critical issues`;
      } else if (score < this.config.warningThreshold) {
        state = 'neutral';
        description = `Design score: ${score}/100 - Improvements suggested`;
      }

      await this.github.repos.createCommitStatus({
        owner,
        repo,
        sha: commitSha,
        state,
        description,
        context: 'fraiseql/design-quality',
      });

      if (!this.config.quiet) {
        console.log(`Status check created: ${state}`);
      }

      return true;
    } catch (error) {
      console.error(`Error creating status check: ${error}`);
      return false;
    }
  }

  /**
   * Check if merge should be blocked
   */
  shouldBlockMerge(): boolean {
    if (!this.auditResponse) {
      return false;
    }

    const score = this.auditResponse.data.overall_score;
    return score < this.config.criticalThreshold;
  }

  /**
   * Get score interpretation
   */
  getScoreInterpretation(): string {
    if (!this.auditResponse) {
      return 'Unknown';
    }

    const score = this.auditResponse.data.overall_score;

    if (score >= 90) {
      return 'Excellent';
    } else if (score >= 75) {
      return 'Good';
    } else if (score >= 60) {
      return 'Fair';
    } else {
      return 'Poor';
    }
  }
}


/**
 * Parse command-line arguments
 */
function parseArgs(): FederationAnalyzerConfig {
  const args = process.argv.slice(2);
  const config: Partial<FederationAnalyzerConfig> = {
    apiEndpoint: 'http://localhost:8080',
    criticalThreshold: 50,
    warningThreshold: 75,
    githubToken: process.env.GITHUB_TOKEN || '',
  };

  for (let i = 0; i < args.length; i++) {
    switch (args[i]) {
      case '--api-endpoint':
        config.apiEndpoint = args[++i];
        break;
      case '--critical-threshold':
        config.criticalThreshold = parseInt(args[++i], 10);
        break;
      case '--warning-threshold':
        config.warningThreshold = parseInt(args[++i], 10);
        break;
      case '--repo':
        config.repo = args[++i];
        break;
      case '--pr':
        config.pr = parseInt(args[++i], 10);
        break;
      case '--fail-if-below':
        config.failIfBelow = parseInt(args[++i], 10);
        break;
      case '--quiet':
        config.quiet = true;
        break;
    }
  }

  return config as FederationAnalyzerConfig;
}


/**
 * Main entry point
 */
async function main(): Promise<number> {
  const config = parseArgs();

  const analyzer = new FederationAnalyzer(config);

  // Load and analyze schema
  const schemaPath = process.argv[2] || 'schema.compiled.json';
  if (!(await analyzer.loadSchema(schemaPath))) {
    return 1;
  }

  if (!(await analyzer.analyze())) {
    return 1;
  }

  // Post PR comment if repo and PR specified
  if (config.repo && config.pr) {
    if (!(await analyzer.postPRComment())) {
      return 1;
    }
  }

  // Check fail threshold
  if (config.failIfBelow) {
    if (analyzer.shouldBlockMerge()) {
      if (!config.quiet) {
        console.error(
          `Design score is below threshold ${config.criticalThreshold}`,
        );
      }
      return 1;
    }
  }

  return 0;
}


export { FederationAnalyzer, parseArgs };

// Run if executed directly
if (import.meta.url === `file://${process.argv[1]}`) {
  main().then(code => process.exit(code));
}
