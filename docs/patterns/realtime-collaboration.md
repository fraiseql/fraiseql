# Real-Time Collaboration with Subscriptions

**Status:** ✅ Production Ready
**Complexity:** ⭐⭐⭐⭐ (Advanced)
**Audience:** Frontend architects, real-time systems engineers
**Reading Time:** 25-30 minutes
**Last Updated:** 2026-02-05

Complete guide to building collaborative tools (like Google Docs, Figma, Notion) with real-time synchronization using FraiseQL subscriptions.

---

## Architecture Overview

```
User A (Editor)          User B (Editor)          User C (Viewer)
    │                         │                         │
    └─────────────────────────┼─────────────────────────┘
                              │
                    ┌─────────┴─────────┐
                    │ WebSocket Server  │
                    │ (FraiseQL Rust)   │
                    └─────────┬─────────┘
                              │
              ┌───────────────┼───────────────┐
              │               │               │
         Mutations       Subscriptions   Presence
         (operations)    (live updates)  (who's online)
              │               │               │
              └───────────────┼───────────────┘
                              │
                    ┌─────────┴─────────┐
                    │   PostgreSQL      │
                    │  - Documents      │
                    │  - Operations     │
                    │  - Changes log    │
                    └───────────────────┘
```

---

## Schema Design

### Document Model

```sql
-- Documents (editable items)
CREATE TABLE documents (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  workspace_id UUID NOT NULL,
  title VARCHAR(255) NOT NULL,
  content_type VARCHAR(50) NOT NULL,  -- text, rich-text, spreadsheet, drawing
  created_by UUID NOT NULL,
  created_at TIMESTAMP DEFAULT NOW(),
  updated_at TIMESTAMP DEFAULT NOW(),
  deleted_at TIMESTAMP,  -- Soft delete

  INDEX idx_workspace_id (workspace_id),
  INDEX idx_updated_at (updated_at)
);

-- Permissions (who can edit/view)
CREATE TABLE document_permissions (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  user_id UUID NOT NULL,
  permission VARCHAR(50) NOT NULL,  -- view, edit, comment, manage
  granted_at TIMESTAMP DEFAULT NOW(),

  UNIQUE(document_id, user_id),
  INDEX idx_document_id (document_id)
);

-- Changes/Operations (for CRDT - Conflict-free Replicated Data Type)
CREATE TABLE document_changes (
  id BIGSERIAL PRIMARY KEY,
  document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  user_id UUID NOT NULL,
  operation JSONB NOT NULL,  -- { type: 'insert', position: 100, content: 'text' }
  vector_clock JSONB NOT NULL,  -- For CRDT: { user_1: 5, user_2: 3 }
  created_at TIMESTAMP DEFAULT NOW(),

  INDEX idx_document_id (document_id),
  INDEX idx_created_at (created_at)
);

-- Activity Stream (for showing what's happening)
CREATE TABLE document_activity (
  id BIGSERIAL PRIMARY KEY,
  document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  user_id UUID NOT NULL,
  activity_type VARCHAR(50) NOT NULL,  -- edit, comment, join, leave
  metadata JSONB,
  created_at TIMESTAMP DEFAULT NOW(),

  INDEX idx_document_id (document_id),
  INDEX idx_created_at (created_at)
);

-- Presence (who's currently editing)
CREATE TABLE document_presence (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  document_id UUID NOT NULL REFERENCES documents(id),
  user_id UUID NOT NULL,
  cursor_position INT,  -- Position in document
  selection_start INT,
  selection_end INT,
  color VARCHAR(7),  -- #FF5733 for user's color
  last_activity TIMESTAMP DEFAULT NOW(),

  INDEX idx_document_id (document_id),
  INDEX idx_last_activity (last_activity)
);

-- Comments
CREATE TABLE comments (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  document_id UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  user_id UUID NOT NULL,
  content TEXT NOT NULL,
  position INT,  -- Where in document (for inline comments)
  resolved BOOLEAN DEFAULT FALSE,
  created_at TIMESTAMP DEFAULT NOW(),
  updated_at TIMESTAMP DEFAULT NOW(),

  INDEX idx_document_id (document_id),
  INDEX idx_resolved (resolved)
);
```

---

## FraiseQL Schema

```python
# collaboration_schema.py
from fraiseql import types
from datetime import datetime
from typing import Optional

@types.object
class Document:
    id: str
    title: str
    content_type: str
    content: str  # Current document state
    created_by: 'User'
    created_at: datetime
    updated_at: datetime
    permissions: list['DocumentPermission']
    current_editors: list['User']  # Who's online
    comments: list['Comment']

@types.object
class DocumentPermission:
    user_id: str
    permission: str  # view, edit, comment, manage
    granted_at: datetime

@types.object
class DocumentChange:
    """Individual operation (for reconstruction)"""
    id: int
    user_id: str
    operation: dict  # type, position, content
    vector_clock: dict  # For CRDT
    created_at: datetime

@types.object
class Presence:
    """Real-time user presence"""
    user_id: str
    cursor_position: int
    selection_start: Optional[int]
    selection_end: Optional[int]
    color: str

@types.object
class DocumentActivity:
    """Activity stream"""
    user_id: str
    activity_type: str  # edit, comment, join, leave
    metadata: dict
    created_at: datetime

@types.object
class Comment:
    id: str
    user_id: str
    content: str
    position: Optional[int]
    resolved: bool
    created_at: datetime
    replies: list['Comment']

@types.object
class Query:
    def document(self, id: str) -> Document:
        """Get document with full content"""
        pass

    def document_changes(
        self,
        document_id: str,
        since_version: int = 0
    ) -> list[DocumentChange]:
        """Get changes since version (for sync)"""
        pass

    def activity_feed(
        self,
        document_id: str,
        limit: int = 50
    ) -> list[DocumentActivity]:
        """Activity stream"""
        pass

@types.object
class Mutation:
    def update_document(
        self,
        document_id: str,
        operation: dict,
        vector_clock: dict
    ) -> DocumentChange:
        """Apply operation (edit)"""
        pass

    def add_comment(
        self,
        document_id: str,
        content: str,
        position: Optional[int] = None
    ) -> Comment:
        """Add comment"""
        pass

    def resolve_comment(self, comment_id: str) -> Comment:
        """Mark comment as resolved"""
        pass

@types.object
class Subscription:
    def document_changes(self, document_id: str) -> DocumentChange:
        """Stream of changes from other users"""
        pass

    def presence(self, document_id: str) -> Presence:
        """Real-time presence updates"""
        pass

    def activity(self, document_id: str) -> DocumentActivity:
        """Real-time activity stream"""
        pass

    def comments(self, document_id: str) -> Comment:
        """New comments"""
        pass
```

---

## Operational Transformation (OT) for Conflict Resolution

### Apply Operation

```python
class OperationTransform:
    @staticmethod
    def apply_operation(text: str, operation: dict) -> str:
        """Apply insert/delete operation"""
        op_type = operation.get('type')
        pos = operation.get('position')
        content = operation.get('content')

        if op_type == 'insert':
            return text[:pos] + content + text[pos:]
        elif op_type == 'delete':
            length = operation.get('length', 1)
            return text[:pos] + text[pos + length:]
        return text

    @staticmethod
    def transform_operations(op1: dict, op2: dict) -> dict:
        """Transform op2 against op1 (for conflict resolution)"""
        # If both insert at same position, use user ID to break tie
        if op1.get('type') == 'insert' and op2.get('type') == 'insert':
            if op1.get('position') == op2.get('position'):
                # Insert later user's content after earlier user's
                return {
                    **op2,
                    'position': op2['position'] + len(op1.get('content', ''))
                }
        # If op2 deletes after op1 insert, adjust position
        elif op1.get('type') == 'insert' and op2.get('type') == 'delete':
            if op2.get('position') > op1.get('position'):
                return {
                    **op2,
                    'position': op2['position'] + len(op1.get('content', ''))
                }
        return op2
```

---

## Real-Time Synchronization Flow

### Client-Side (React)

```typescript
import { useQuery, useMutation, useSubscription, gql } from '@apollo/client';
import { useCallback, useRef, useState } from 'react';

const DOCUMENT_QUERY = gql`
  query GetDocument($id: ID!) {
    document(id: $id) {
      id
      title
      content
      currentEditors { id email }
      comments { id content resolved }
    }
  }
`;

const UPDATE_DOCUMENT = gql`
  mutation UpdateDocument($docId: ID!, $operation: JSON!, $vectorClock: JSON!) {
    updateDocument(documentId: $docId, operation: $operation, vectorClock: $vectorClock) {
      id
      operation
      createdAt
    }
  }
`;

const DOCUMENT_CHANGES_SUB = gql`
  subscription OnDocumentChanges($docId: ID!) {
    documentChanges(documentId: $docId) {
      id
      userId
      operation
      vectorClock
      createdAt
    }
  }
`;

const PRESENCE_SUB = gql`
  subscription OnPresence($docId: ID!) {
    presence(documentId: $docId) {
      userId
      cursorPosition
      color
    }
  }
`;

export function CollaborativeEditor({ documentId }: { documentId: string }) {
  const editorRef = useRef<HTMLDivElement>(null);
  const [content, setContent] = useState('');
  const [vectorClock, setVectorClock] = useState<Record<string, number>>({});
  const [editors, setEditors] = useState<any[]>([]);
  const userId = getCurrentUserId();

  // Fetch initial document
  const { data: docData } = useQuery(DOCUMENT_QUERY, {
    variables: { id: documentId },
  });

  // Listen for changes from other users
  const { data: changesData } = useSubscription(DOCUMENT_CHANGES_SUB, {
    variables: { docId: documentId },
  });

  // Listen for presence updates
  const { data: presenceData } = useSubscription(PRESENCE_SUB, {
    variables: { docId: documentId },
  });

  const [updateDocument] = useMutation(UPDATE_DOCUMENT);

  // Initialize
  useEffect(() => {
    if (docData?.document) {
      setContent(docData.document.content);
      setEditors(docData.document.currentEditors);
    }
  }, [docData]);

  // Apply remote changes
  useEffect(() => {
    if (changesData?.documentChanges) {
      const change = changesData.documentChanges;
      const newContent = applyOperation(content, change.operation);
      setContent(newContent);

      // Update vector clock
      setVectorClock(prev => ({
        ...prev,
        [change.userId]: (prev[change.userId] || 0) + 1,
      }));
    }
  }, [changesData]);

  // Update presence indicators
  useEffect(() => {
    if (presenceData?.presence) {
      renderCursorPosition(presenceData.presence);
    }
  }, [presenceData]);

  // Handle local edits
  const handleChange = useCallback(
    async (e: React.ChangeEvent<HTMLDivElement>) => {
      const newContent = e.currentTarget.textContent || '';
      const operation = detectOperation(content, newContent);

      // Update local state immediately
      setContent(newContent);

      // Increment our vector clock
      const newClock = {
        ...vectorClock,
        [userId]: (vectorClock[userId] || 0) + 1,
      };
      setVectorClock(newClock);

      // Send to server
      await updateDocument({
        variables: {
          docId: documentId,
          operation,
          vectorClock: newClock,
        },
      });
    },
    [content, vectorClock, documentId, updateDocument, userId]
  );

  return (
    <div>
      <h1>{docData?.document?.title}</h1>

      {/* Show current editors */}
      <div className="editors">
        {editors.map(editor => (
          <span key={editor.id} title={editor.email}>👤</span>
        ))}
      </div>

      {/* Editor with collaborative cursors */}
      <div
        ref={editorRef}
        contentEditable
        onInput={handleChange}
        className="editor"
      >
        {content}
      </div>

      {/* Comments sidebar */}
      <CommentsSidebar documentId={documentId} />
    </div>
  );
}

function detectOperation(oldContent: string, newContent: string) {
  // Find the difference
  const minLen = Math.min(oldContent.length, newContent.length);
  let start = 0;
  while (start < minLen && oldContent[start] === newContent[start]) {
    start++;
  }

  if (newContent.length > oldContent.length) {
    // Insert
    return {
      type: 'insert',
      position: start,
      content: newContent.slice(start, newContent.length - (oldContent.length - start)),
    };
  } else {
    // Delete
    return {
      type: 'delete',
      position: start,
      length: oldContent.length - newContent.length,
    };
  }
}

function applyOperation(text: string, operation: any): string {
  if (operation.type === 'insert') {
    return text.slice(0, operation.position) +
           operation.content +
           text.slice(operation.position);
  } else if (operation.type === 'delete') {
    return text.slice(0, operation.position) +
           text.slice(operation.position + operation.length);
  }
  return text;
}

function renderCursorPosition(presence: any) {
  // Render remote user's cursor in document
  const cursor = document.querySelector(`[data-user-id="${presence.userId}"]`);
  if (cursor) {
    cursor.style.left = presence.cursorPosition + 'px';
  }
}
```

---

## Activity Feed & Presence

### Track Activity

```sql
-- Trigger to log activity
CREATE OR REPLACE FUNCTION log_activity() RETURNS TRIGGER AS $$
BEGIN
  INSERT INTO document_activity (
    document_id,
    user_id,
    activity_type,
    metadata
  ) VALUES (
    NEW.document_id,
    NEW.user_id,
    'edit',
    jsonb_build_object(
      'operation_type', NEW.operation->>'type',
      'operation_id', NEW.id
    )
  );
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER document_changes_activity
AFTER INSERT ON document_changes
FOR EACH ROW
EXECUTE FUNCTION log_activity();
```

### Presence Management

```typescript
// Update presence every time cursor moves
const handleCursorMove = useDebouncedCallback((position: number) => {
  updatePresence({
    variables: {
      documentId,
      cursorPosition: position,
      selectionStart: selection.start,
      selectionEnd: selection.end,
    },
  });
}, 500); // Debounce to avoid spam

// Clean up on unmount (user left)
useEffect(() => {
  return () => {
    removePresence({
      variables: { documentId },
    });
  };
}, [documentId]);
```

---

## Comments & Threading

```typescript
const ADD_COMMENT = gql`
  mutation AddComment($docId: ID!, $content: String!, $position: Int) {
    addComment(documentId: $docId, content: $content, position: $position) {
      id
      content
      userId
      createdAt
    }
  }
`;

export function CommentsSidebar({ documentId }: { documentId: string }) {
  const [selectedPosition, setSelectedPosition] = useState<number | null>(null);
  const { data } = useQuery(DOCUMENT_QUERY, { variables: { id: documentId } });
  const [addComment] = useMutation(ADD_COMMENT);

  const handleHighlightText = (start: number, end: number) => {
    setSelectedPosition(start);
  };

  const handleSubmitComment = async (content: string) => {
    await addComment({
      variables: {
        docId: documentId,
        content,
        position: selectedPosition,
      },
    });
    setSelectedPosition(null);
  };

  return (
    <aside className="comments-sidebar">
      {data?.document?.comments?.map(comment => (
        <CommentThread key={comment.id} comment={comment} />
      ))}
      {selectedPosition !== null && (
        <CommentForm onSubmit={handleSubmitComment} />
      )}
    </aside>
  );
}
```

---

## Conflict Resolution Strategy

For documents with concurrent edits:

1. **Vector Clocks** - Track causality
2. **Last-Write-Wins** - If concurrent edits at same position
3. **User ID Tiebreaker** - Sort by user ID for determinism
4. **Operational Transform** - Adjust operations based on order

```python
def resolve_conflict(op1: dict, op2: dict, user1_id: str, user2_id: str) -> tuple:
    """Return (transformed_op1, transformed_op2)"""

    # If both inserting at same position
    if (op1['type'] == 'insert' and op2['type'] == 'insert' and
        op1['position'] == op2['position']):

        # User with earlier ID gets their insert first
        if user1_id < user2_id:
            op2_new = {
                **op2,
                'position': op2['position'] + len(op1['content'])
            }
            return (op1, op2_new)
        else:
            op1_new = {
                **op1,
                'position': op1['position'] + len(op2['content'])
            }
            return (op1_new, op2)

    # Handle other cases...
    return (op1, op2)
```

---

## Testing Real-Time Collaboration

```typescript
describe('Collaborative Editing', () => {
  it('should merge concurrent edits', async () => {
    const user1Changes = [
      { type: 'insert', position: 0, content: 'Hello' },
    ];
    const user2Changes = [
      { type: 'insert', position: 0, content: 'World' },
    ];

    const result = await resolveConflict(user1Changes, user2Changes);

    // Both changes should be preserved in final content
    expect(result).toContain('Hello');
    expect(result).toContain('World');
  });

  it('should update presence correctly', async () => {
    const presence = usePresenceHook('doc123');

    expect(presence.editors).toHaveLength(0);
    presence.join('user1');
    expect(presence.editors).toHaveLength(1);
    presence.leave('user1');
    expect(presence.editors).toHaveLength(0);
  });

  it('should maintain comment threads', async () => {
    const comment = await addComment({
      documentId: 'doc123',
      content: 'Fix this typo',
      position: 100,
    });

    expect(comment.id).toBeDefined();
    expect(comment.position).toBe(100);

    const reply = await replyToComment({
      commentId: comment.id,
      content: 'Already fixed!',
    });

    expect(reply.parentCommentId).toBe(comment.id);
  });
});
```

---

## Performance Optimization

- **Debounce presence updates** - Only send every 500ms
- **Batch operations** - Group changes from same user
- **Archive old changes** - Delete changes older than 30 days
- **Snapshot documents** - Store full content snapshot periodically
- **Compress change log** - Compact operations into final state

---

## See Also

**Related Patterns:**
- [Multi-Tenant SaaS](./saas-multi-tenant.md) - Document ownership/permissions
- [Activity Feeds in Social Networks](./social-network.md) - Similar patterns

**Real-Time Features:**
- [Real-Time Patterns](../guides/PATTERNS.md)
- [Subscriptions & WebSockets](../guides/clients/README.md)

**Production Deployment:**
- [Production Deployment](../guides/production-deployment.md)
- [Observability & Monitoring](../guides/observability.md)

---

**Last Updated:** 2026-02-05
**Version:** v2.0.0-alpha.1
