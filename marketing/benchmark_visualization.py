#!/usr/bin/env python3
"""Generate benchmark visualization charts for FraiseQL marketing materials"""

import matplotlib.pyplot as plt
import numpy as np

# Set style for professional appearance
plt.style.use("seaborn-v0_8-darkgrid")


def create_performance_comparison():
    """Create a bar chart comparing FraiseQL vs traditional stack performance"""
    # Data from benchmarks
    categories = ["Simple Query", "Complex Query", "Memory Usage"]
    fraiseql = [3.8, 18, 50]  # ms, ms, MB
    java_orm = [10, 385, 300]  # ms, ms, MB

    # Calculate improvement percentages
    improvements = [(java_orm[i] - fraiseql[i]) / java_orm[i] * 100 for i in range(len(fraiseql))]

    x = np.arange(len(categories))
    width = 0.35

    fig, ax = plt.subplots(figsize=(10, 6))

    # Create bars
    bars1 = ax.bar(x - width / 2, fraiseql, width, label="FraiseQL", color="#e74c3c", alpha=0.8)
    bars2 = ax.bar(x + width / 2, java_orm, width, label="Java + ORM", color="#95a5a6", alpha=0.8)

    # Add improvement percentages on top
    for _i, (bar1, improvement) in enumerate(zip(bars1, improvements, strict=False)):
        height = bar1.get_height()
        ax.text(
            bar1.get_x() + bar1.get_width() / 2.0,
            height + 5,
            f"-{improvement:.0f}%",
            ha="center",
            va="bottom",
            fontsize=11,
            fontweight="bold",
            color="#27ae60",
        )

    # Customize the plot
    ax.set_xlabel("Type de requête", fontsize=12)
    ax.set_ylabel("Temps de réponse (ms) / Mémoire (MB)", fontsize=12)
    ax.set_title(
        "Performance FraiseQL vs Stack Traditionnelle",
        fontsize=14,
        fontweight="bold",
        pad=20,
    )
    ax.set_xticks(x)
    ax.set_xticklabels(categories)
    ax.legend(loc="upper right")

    # Add value labels on bars
    for bars in [bars1, bars2]:
        for bar in bars:
            height = bar.get_height()
            ax.text(
                bar.get_x() + bar.get_width() / 2.0,
                height / 2,
                f"{height:.0f}",
                ha="center",
                va="center",
                fontsize=10,
            )

    # Add grid for better readability
    ax.grid(True, axis="y", alpha=0.3)

    # Adjust layout
    plt.tight_layout()

    # Save the figure
    plt.savefig(
        "marketing/performance_comparison.png",
        dpi=300,
        bbox_inches="tight",
        facecolor="white",
    )
    plt.savefig("marketing/performance_comparison.svg", bbox_inches="tight", facecolor="white")

    return fig


def create_detailed_benchmark():
    """Create a more detailed benchmark visualization"""
    fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(14, 6))

    # Response Time Comparison
    frameworks = ["FraiseQL", "FraiseQL\n+TurboRouter", "Hasura", "PostGraphile", "Java+JPA"]
    simple_query = [3.8, 3.2, 3.1, 3.5, 10]
    complex_query = [18, 16, 14, 16, 385]

    x = np.arange(len(frameworks))
    width = 0.35

    bars1 = ax1.bar(
        x - width / 2,
        simple_query,
        width,
        label="Requête simple",
        color="#3498db",
        alpha=0.8,
    )
    bars2 = ax1.bar(
        x + width / 2,
        complex_query,
        width,
        label="Requête complexe",
        color="#e74c3c",
        alpha=0.8,
    )

    ax1.set_ylabel("Temps de réponse (ms)", fontsize=12)
    ax1.set_title("Temps de Réponse par Framework", fontsize=14, fontweight="bold")
    ax1.set_xticks(x)
    ax1.set_xticklabels(frameworks, rotation=15, ha="right")
    ax1.legend()
    ax1.set_yscale("log")  # Log scale for better visibility

    # Add value labels
    for bars in [bars1, bars2]:
        for bar in bars:
            height = bar.get_height()
            ax1.text(
                bar.get_x() + bar.get_width() / 2.0,
                height,
                f"{height:.1f}",
                ha="center",
                va="bottom",
                fontsize=9,
            )

    # Throughput Comparison
    frameworks_throughput = ["FraiseQL", "Strawberry", "Flask+\nGraphQL", "FastAPI+\nGraphQL"]
    throughput = [2632, 1149, 892, 1876]  # requests/second

    bars = ax2.bar(
        frameworks_throughput,
        throughput,
        color=["#e74c3c", "#f39c12", "#95a5a6", "#3498db"],
        alpha=0.8,
    )

    ax2.set_ylabel("Requêtes par seconde", fontsize=12)
    ax2.set_title("Débit (Throughput)", fontsize=14, fontweight="bold")
    ax2.set_ylim(0, 3000)

    # Add value labels
    for bar in bars:
        height = bar.get_height()
        ax2.text(
            bar.get_x() + bar.get_width() / 2.0,
            height + 50,
            f"{height:.0f}",
            ha="center",
            va="bottom",
            fontsize=10,
            fontweight="bold",
        )

    # Add horizontal line for reference
    ax2.axhline(y=2000, color="gray", linestyle="--", alpha=0.5, label="Objectif 2000 req/s")
    ax2.legend()

    # Add grid
    for ax in [ax1, ax2]:
        ax.grid(True, axis="y", alpha=0.3)

    plt.suptitle("Benchmarks FraiseQL - Performance Réelle", fontsize=16, fontweight="bold", y=1.02)
    plt.tight_layout()

    # Save the figure
    plt.savefig(
        "marketing/detailed_benchmarks.png",
        dpi=300,
        bbox_inches="tight",
        facecolor="white",
    )
    plt.savefig("marketing/detailed_benchmarks.svg", bbox_inches="tight", facecolor="white")

    return fig


def create_architecture_benefits():
    """Create a visual showing architecture benefits"""
    fig, ax = plt.subplots(figsize=(10, 8))

    # Data for radar chart
    categories = [
        "Performance\n(req/s)",
        "Mémoire\n(efficacité)",
        "Simplicité\n(lignes de code)",
        "Maintenabilité",
        "Coût Cloud\n(économies)",
        "IA-Friendly",
    ]

    # Scores out of 100
    fraiseql_scores = [85, 90, 95, 80, 85, 95]
    traditional_scores = [40, 30, 40, 60, 40, 50]

    # Number of variables
    num_vars = len(categories)

    # Compute angle for each axis
    angles = np.linspace(0, 2 * np.pi, num_vars, endpoint=False).tolist()

    # Complete the circle
    fraiseql_scores += fraiseql_scores[:1]
    traditional_scores += traditional_scores[:1]
    angles += angles[:1]

    # Initialize the plot
    ax = plt.subplot(111, polar=True)

    # Draw the outlines
    ax.plot(angles, fraiseql_scores, "o-", linewidth=2, label="FraiseQL", color="#e74c3c")
    ax.fill(angles, fraiseql_scores, alpha=0.25, color="#e74c3c")

    ax.plot(
        angles,
        traditional_scores,
        "o-",
        linewidth=2,
        label="Stack Traditionnelle",
        color="#95a5a6",
    )
    ax.fill(angles, traditional_scores, alpha=0.25, color="#95a5a6")

    # Fix axis to go in the right order and start at 12 o'clock
    ax.set_theta_offset(np.pi / 2)
    ax.set_theta_direction(-1)

    # Draw axis lines for each angle and label
    ax.set_xticks(angles[:-1])
    ax.set_xticklabels(categories, size=11)

    # Set y-axis limits and labels
    ax.set_ylim(0, 100)
    ax.set_yticks([20, 40, 60, 80, 100])
    ax.set_yticklabels(["20", "40", "60", "80", "100"], size=9)

    # Add title and legend
    plt.title("Avantages Architecturaux de FraiseQL", size=16, fontweight="bold", pad=30)
    plt.legend(loc="upper right", bbox_to_anchor=(1.2, 1.1))

    # Save the figure
    plt.savefig(
        "marketing/architecture_benefits.png",
        dpi=300,
        bbox_inches="tight",
        facecolor="white",
    )
    plt.savefig("marketing/architecture_benefits.svg", bbox_inches="tight", facecolor="white")

    return fig


def create_token_usage_comparison():
    """Create a bar chart showing LLM token usage comparison"""
    fig, ax = plt.subplots(figsize=(10, 6))

    # Data from LLM architecture documentation
    components = ["Types", "Queries", "Mutations", "Resolvers", "Total"]
    fraiseql_tokens = [800, 1200, 800, 400, 3200]
    traditional_tokens = [1500, 2000, 1500, 3000, 8000]

    x = np.arange(len(components))
    width = 0.35

    bars1 = ax.bar(
        x - width / 2,
        fraiseql_tokens,
        width,
        label="FraiseQL",
        color="#e74c3c",
        alpha=0.8,
    )
    bars2 = ax.bar(
        x + width / 2,
        traditional_tokens,
        width,
        label="Stack Traditionnelle",
        color="#95a5a6",
        alpha=0.8,
    )

    # Highlight the total difference
    total_reduction = (traditional_tokens[-1] - fraiseql_tokens[-1]) / traditional_tokens[-1] * 100

    # Add value labels
    for bars in [bars1, bars2]:
        for _i, bar in enumerate(bars):
            height = bar.get_height()
            ax.text(
                bar.get_x() + bar.get_width() / 2.0,
                height + 50,
                f"{height:,}",
                ha="center",
                va="bottom",
                fontsize=10,
            )

    # Add reduction percentage on total
    ax.text(
        bars1[-1].get_x() + bars1[-1].get_width() / 2.0,
        bars1[-1].get_height() + 500,
        f"-{total_reduction:.0f}%",
        ha="center",
        va="bottom",
        fontsize=14,
        fontweight="bold",
        color="#27ae60",
        bbox={"boxstyle": "round,pad=0.3", "facecolor": "white", "edgecolor": "#27ae60"},
    )

    # Customize
    ax.set_xlabel("Composants", fontsize=12)
    ax.set_ylabel("Tokens LLM nécessaires", fontsize=12)
    ax.set_title("Réduction de Tokens pour Génération IA", fontsize=14, fontweight="bold", pad=20)
    ax.set_xticks(x)
    ax.set_xticklabels(components)
    ax.legend()
    ax.grid(True, axis="y", alpha=0.3)

    # Add note
    fig.text(
        0.5,
        -0.05,
        "Basé sur un blog API avec 5 types, 10 queries, 5 mutations",
        ha="center",
        fontsize=10,
        style="italic",
        color="gray",
    )

    plt.tight_layout()

    # Save
    plt.savefig(
        "marketing/token_usage_comparison.png",
        dpi=300,
        bbox_inches="tight",
        facecolor="white",
    )
    plt.savefig("marketing/token_usage_comparison.svg", bbox_inches="tight", facecolor="white")

    return fig


if __name__ == "__main__":
    # Create all visualizations

    # Create the main performance comparison
    fig1 = create_performance_comparison()

    # Create detailed benchmarks
    fig2 = create_detailed_benchmark()

    # Create architecture benefits radar
    fig3 = create_architecture_benefits()

    # Create token usage comparison
    fig4 = create_token_usage_comparison()

    # Show the plots (optional)
    # plt.show()
