/**
 * FraiseQL Tutorial Application
 * Handles chapter navigation, query execution, and progress tracking
 */

// State
let chapters = [];
let currentChapterId = 1;
let completedChapters = new Set(JSON.parse(localStorage.getItem('completedChapters') || '[]'));

// DOM Elements
const chapterListEl = document.getElementById('chapters');
const lessonContentEl = document.getElementById('lesson-content');
const queryExecutorEl = document.getElementById('query-executor');
const queryInputEl = document.getElementById('query-input');
const executeBtn = document.getElementById('execute-btn');
const clearBtn = document.getElementById('clear-btn');
const resultOutputEl = document.getElementById('result-output');
const sqlOutputEl = document.getElementById('sql-output');
const timingOutputEl = document.getElementById('timing-output');
const prevBtnEl = document.getElementById('prev-btn');
const nextBtnEl = document.getElementById('next-btn');
const chapterIndicatorEl = document.getElementById('chapter-indicator');
const progressFillEl = document.getElementById('progress-fill');
const progressTextEl = document.getElementById('progress-text');
const schemaExplorerEl = document.getElementById('schema-explorer');

// Initialize
async function init() {
    try {
        // Fetch chapters
        const response = await fetch('/api/chapters');
        chapters = await response.json();

        // Render chapter list
        renderChapterList();

        // Load first chapter
        await loadChapter(1);

        // Set up event listeners
        setupEventListeners();
    } catch (error) {
        console.error('Failed to initialize tutorial:', error);
        lessonContentEl.innerHTML = '<p style="color: red;">Failed to load tutorial. Please refresh.</p>';
    }
}

// Render chapter list in sidebar
function renderChapterList() {
    const html = chapters.map(chapter => {
        const isActive = chapter.id === currentChapterId ? 'active' : '';
        const isCompleted = completedChapters.has(chapter.id);
        const checkmark = isCompleted ? '✓ ' : '';
        return `
            <li>
                <button class="chapter-btn ${isActive}" data-chapter-id="${chapter.id}">
                    ${checkmark}${chapter.title}
                </button>
            </li>
        `;
    }).join('');

    chapterListEl.innerHTML = html;

    // Add click handlers
    chapterListEl.querySelectorAll('.chapter-btn').forEach(btn => {
        btn.addEventListener('click', () => {
            const chapterId = parseInt(btn.dataset.chapterId);
            loadChapter(chapterId);
        });
    });
}

// Load and display chapter
async function loadChapter(chapterId) {
    try {
        const response = await fetch(`/api/chapters/${chapterId}`);
        if (!response.ok) throw new Error('Chapter not found');

        const chapter = await response.json();
        currentChapterId = chapterId;

        // Update sidebar
        document.querySelectorAll('.chapter-btn').forEach(btn => {
            btn.classList.toggle('active', parseInt(btn.dataset.chapterId) === chapterId);
        });

        // Render content
        renderChapterContent(chapter);

        // Update navigation
        updateNavigation();

        // Mark as completed if has sample query
        if (chapter.sampleQuery) {
            completedChapters.add(chapterId);
            saveProgress();
        }

        // Scroll to top
        lessonContentEl.scrollTop = 0;
    } catch (error) {
        console.error('Failed to load chapter:', error);
        lessonContentEl.innerHTML = '<p style="color: red;">Failed to load chapter.</p>';
    }
}

// Render chapter content
function renderChapterContent(chapter) {
    const contentHtml = markdownToHtml(chapter.content);

    lessonContentEl.innerHTML = contentHtml;

    // Show/hide query executor
    if (chapter.sampleQuery) {
        queryExecutorEl.style.display = 'block';
        queryInputEl.value = chapter.sampleQuery;
        resultOutputEl.textContent = 'Ready to execute...';
        executionInfoEl.textContent = 'Click "Execute Query" to run';
    } else {
        queryExecutorEl.style.display = 'none';
    }

    updateProgress();
}

// Simple markdown to HTML converter
function markdownToHtml(markdown) {
    let html = markdown;

    // Handle headers
    html = html.replace(/^# (.*?)$/gm, '<h1>$1</h1>');
    html = html.replace(/^## (.*?)$/gm, '<h2>$1</h2>');
    html = html.replace(/^### (.*?)$/gm, '<h3>$1</h3>');

    // Handle code blocks first
    html = html.replace(/```(.*?)\n([\s\S]*?)```/g, (match, lang, code) => {
        return `<pre><code>${escapeHtml(code.trim())}</code></pre>`;
    });

    // Handle inline code
    html = html.replace(/`([^`]+)`/g, '<code>$1</code>');

    // Handle bold and italic
    html = html.replace(/\*\*(.*?)\*\*/g, '<strong>$1</strong>');
    html = html.replace(/\*(.*?)\*/g, '<em>$1</em>');

    // Handle tables
    html = html.replace(/^\|(.+)\n\|[-:\s|]+\n((?:\|.+\n?)*)/gm, (match, header, rows) => {
        const headerCells = header.split('|').filter(c => c.trim());
        const headerHtml = headerCells.map(c => `<th>${c.trim()}</th>`).join('');

        const rowsArray = rows.split('\n').filter(r => r.trim());
        const rowsHtml = rowsArray.map(row => {
            const cells = row.split('|').filter(c => c.trim());
            return '<tr>' + cells.map(c => `<td>${c.trim()}</td>`).join('') + '</tr>';
        }).join('');

        return `<table><thead><tr>${headerHtml}</tr></thead><tbody>${rowsHtml}</tbody></table>`;
    });

    // Handle lists
    html = html.replace(/^- (.*?)$/gm, '<li>$1</li>');
    html = html.replace(/(<li>[\s\S]*?<\/li>)/s, '<ul>$1</ul>');

    // Handle paragraphs
    html = html.split(/\n\n+/).map(paragraph => {
        // Don't wrap HTML elements
        if (paragraph.match(/^<[a-z]/i) || paragraph.match(/<\/[a-z]/i)) {
            return paragraph;
        }
        // Join single lines back together
        const lines = paragraph.split('\n');
        return lines.every(l => !l.match(/^<[a-z]/i)) ? `<p>${lines.join(' ')}</p>` : lines.join('\n');
    }).join('');

    // Handle line breaks in preformatted text
    html = html.replace(/\n(?!<)/g, '<br>');

    return html;
}

function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

// Update navigation buttons
function updateNavigation() {
    prevBtnEl.disabled = currentChapterId === 1;
    nextBtnEl.disabled = currentChapterId === chapters.length;

    const position = `Chapter ${currentChapterId} of ${chapters.length}`;
    chapterIndicatorEl.textContent = position;
}

// Update progress bar
function updateProgress() {
    const progress = (completedChapters.size / chapters.length) * 100;
    progressFillEl.style.width = `${progress}%`;
    progressTextEl.textContent = `${Math.round(progress)}% Complete`;
}

// Save progress to localStorage
function saveProgress() {
    localStorage.setItem('completedChapters', JSON.stringify([...completedChapters]));
    renderChapterList();
    updateProgress();
}

// Execute GraphQL query
async function executeQuery() {
    const query = queryInputEl.value.trim();

    if (!query) {
        resultOutputEl.textContent = 'Please enter a query';
        return;
    }

    try {
        resultOutputEl.textContent = 'Executing...';
        executeBtn.disabled = true;

        const startTime = performance.now();
        const response = await fetch('/api/execute', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ query }),
        });

        const result = await response.json();
        const endTime = performance.now();
        const duration = (endTime - startTime).toFixed(2);

        // Display result
        resultOutputEl.textContent = JSON.stringify(result, null, 2);

        // Display compiled SQL (simulated based on query analysis)
        const compiledSql = generateSimulatedSql(query);
        sqlOutputEl.textContent = compiledSql;

        // Display timing info
        const timingInfo = {
            'Execution Time': `${duration} ms`,
            'Status': result.errors ? '❌ Error' : '✅ Success',
            'Timestamp': new Date().toLocaleTimeString(),
            'Records Returned': countResults(result.data),
            'Query Complexity': analyzeQueryComplexity(query),
        };
        timingOutputEl.textContent = formatTimingInfo(timingInfo);

        // Mark chapter as completed
        completedChapters.add(currentChapterId);
        saveProgress();

        // Load schema explorer
        loadSchemaExplorer();
    } catch (error) {
        resultOutputEl.textContent = `Error: ${error.message}`;
        sqlOutputEl.textContent = 'Error executing query';
        timingOutputEl.textContent = `Failed: ${error.message}`;
    } finally {
        executeBtn.disabled = false;
    }
}

// Simulate SQL compilation from GraphQL query (for educational purposes)
function generateSimulatedSql(graphqlQuery) {
    // This is a simplified simulation for educational purposes
    // Real FraiseQL generates actual optimized SQL

    if (graphqlQuery.includes('users')) {
        if (graphqlQuery.includes('posts') || graphqlQuery.includes('author')) {
            return `-- Pre-compiled SQL (optimized with JOIN)
SELECT
  u.id, u.name, u.email, u.created_at,
  p.id AS post_id, p.title, p.content, p.created_at AS post_created_at
FROM users u
LEFT JOIN posts p ON p.author_id = u.id
ORDER BY u.id, p.created_at DESC
LIMIT 10;`;
        }
        return `-- Pre-compiled SQL
SELECT id, name, email, created_at
FROM users
LIMIT 10;`;
    } else if (graphqlQuery.includes('posts')) {
        return `-- Pre-compiled SQL
SELECT id, title, content, author_id, created_at
FROM posts
LIMIT 20;`;
    }

    return `-- Query analysis unavailable`;
}

// Count returned results
function countResults(data) {
    if (!data) return '0';
    if (Array.isArray(data)) return data.length.toString();

    // Try to find array in nested data
    for (const key in data) {
        if (Array.isArray(data[key])) {
            return data[key].length.toString();
        }
    }

    return '1';
}

// Analyze query complexity
function analyzeQueryComplexity(query) {
    const fieldCount = (query.match(/{/g) || []).length;
    if (fieldCount <= 3) return 'Simple';
    if (fieldCount <= 6) return 'Moderate';
    return 'Complex';
}

// Format timing info for display
function formatTimingInfo(info) {
    return Object.entries(info)
        .map(([key, value]) => `${key}: ${value}`)
        .join('\n');
}

// Load and display schema explorer
async function loadSchemaExplorer() {
    try {
        const response = await fetch('/api/schema/types');
        const data = await response.json();

        if (data.types && data.types.length > 0) {
            const typesHtml = data.types
                .slice(0, 5) // Show top 5 types
                .map(type => `
                    <div class="schema-type" onclick="loadTypeDetails('${type.name}')">
                        <div class="schema-type-name">${type.name}</div>
                        <div class="schema-type-kind">${type.kind}</div>
                    </div>
                `)
                .join('');

            schemaExplorerEl.innerHTML = typesHtml || '<p class="text-muted">No types available</p>';
        }
    } catch (error) {
        schemaExplorerEl.innerHTML = '<p class="text-muted">Schema explorer unavailable</p>';
    }
}

// Load type details (unused for now, but available for future enhancement)
async function loadTypeDetails(typeName) {
    try {
        const response = await fetch(`/api/schema/type/${typeName}`);
        const data = await response.json();
        console.log('Type details:', data);
    } catch (error) {
        console.error('Failed to load type details:', error);
    }
}

// Setup event listeners
function setupEventListeners() {
    // Query executor
    executeBtn.addEventListener('click', executeQuery);
    clearBtn.addEventListener('click', () => {
        queryInputEl.value = '';
        queryInputEl.focus();
    });

    queryInputEl.addEventListener('keydown', (e) => {
        if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
            executeQuery();
        }
    });

    // Tab switching
    document.querySelectorAll('.tab-btn').forEach(btn => {
        btn.addEventListener('click', (e) => {
            const tabName = e.target.dataset.tab;
            switchTab(tabName);
        });
    });

    // Navigation
    prevBtnEl.addEventListener('click', () => {
        if (currentChapterId > 1) {
            loadChapter(currentChapterId - 1);
        }
    });

    nextBtnEl.addEventListener('click', () => {
        if (currentChapterId < chapters.length) {
            loadChapter(currentChapterId + 1);
        }
    });

    // Keyboard navigation
    document.addEventListener('keydown', (e) => {
        if (e.key === 'ArrowLeft' && currentChapterId > 1) {
            loadChapter(currentChapterId - 1);
        } else if (e.key === 'ArrowRight' && currentChapterId < chapters.length) {
            loadChapter(currentChapterId + 1);
        }
    });
}

// Switch between result tabs
function switchTab(tabName) {
    // Update tab buttons
    document.querySelectorAll('.tab-btn').forEach(btn => {
        btn.classList.toggle('active', btn.dataset.tab === tabName);
    });

    // Update tab content
    document.querySelectorAll('.tab-pane').forEach(pane => {
        pane.classList.toggle('active', pane.id === `tab-${tabName}`);
    });
}

// Start the app
document.addEventListener('DOMContentLoaded', init);
