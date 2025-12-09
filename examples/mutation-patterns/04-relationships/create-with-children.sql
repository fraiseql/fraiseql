-- ============================================================================
-- Pattern: Create Parent with Children
-- ============================================================================
-- Use Case: Create related records atomically in single mutation
-- Benefits: Atomic transactions, referential integrity, reduced round-trips
--
-- This example shows:
-- - Creating parent and children in one transaction
-- - Handling foreign key relationships
-- - Returning nested results
-- - Using CASCADE field for related data
-- ============================================================================

CREATE OR REPLACE FUNCTION create_blog_post_with_tags(input_payload jsonb)
RETURNS mutation_response AS $$
DECLARE
    result mutation_response;
    post_record record;
    tag_record record;
    tag_name text;
    tags_data jsonb;
    created_tags jsonb := '[]'::jsonb;
    author_id uuid := (input_payload->>'author_id')::uuid;
    post_title text := input_payload->>'title';
    post_content text := input_payload->>'content';
BEGIN
    -- ========================================================================
    -- Validate Input
    -- ========================================================================

    IF author_id IS NULL THEN
        result.status := 'failed:validation';
        result.message := 'Author ID is required';
        RETURN result;
    END IF;

    IF post_title IS NULL OR trim(post_title) = '' THEN
        result.status := 'failed:validation';
        result.message := 'Title is required';
        RETURN result;
    END IF;

    -- Verify author exists
    IF NOT EXISTS (SELECT 1 FROM users WHERE id = author_id) THEN
        result.status := 'not_found:author';
        result.message := 'Author not found';
        RETURN result;
    END IF;

    -- ========================================================================
    -- Create Parent: Post
    -- ========================================================================

    INSERT INTO posts (author_id, title, content, status)
    VALUES (author_id, post_title, post_content, 'draft')
    RETURNING * INTO post_record;

    -- ========================================================================
    -- Create Children: Tags
    -- ========================================================================

    tags_data := input_payload->'tags';

    IF tags_data IS NOT NULL AND jsonb_typeof(tags_data) = 'array' THEN
        FOR tag_name IN
            SELECT jsonb_array_elements_text(tags_data)
        LOOP
            -- Create or get existing tag
            INSERT INTO tags (name, slug)
            VALUES (
                tag_name,
                lower(regexp_replace(tag_name, '[^a-zA-Z0-9]+', '-', 'g'))
            )
            ON CONFLICT (slug) DO UPDATE SET name = EXCLUDED.name
            RETURNING * INTO tag_record;

            -- Link post to tag
            INSERT INTO post_tags (post_id, tag_id)
            VALUES (post_record.id, tag_record.id)
            ON CONFLICT DO NOTHING;

            -- Collect created/linked tags
            created_tags := created_tags || to_jsonb(tag_record);
        END LOOP;
    END IF;

    -- ========================================================================
    -- Success Response with CASCADE
    -- ========================================================================

    result.status := 'created';
    result.message := format('Post created with %s tag(s)', jsonb_array_length(created_tags));
    result.entity := row_to_json(post_record);
    result.entity_id := post_record.id::text;
    result.entity_type := 'Post';

    -- Include created children in CASCADE
    result.cascade := jsonb_build_object(
        'created', jsonb_build_object(
            'tags', created_tags
        )
    );

    RETURN result;

EXCEPTION
    WHEN OTHERS THEN
        result.status := 'failed:error';
        result.message := SQLERRM;
        RETURN result;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- Usage Examples
-- ============================================================================

-- Create post with tags
SELECT * FROM create_blog_post_with_tags('{
    "author_id": "550e8400-e29b-41d4-a716-446655440000",
    "title": "Getting Started with FraiseQL",
    "content": "FraiseQL makes GraphQL development easier...",
    "tags": ["graphql", "postgresql", "python"]
}'::jsonb);
-- Returns: status='created', entity=post, cascade.created.tags=[3 tags]

-- Create post without tags
SELECT * FROM create_blog_post_with_tags('{
    "author_id": "550e8400-e29b-41d4-a716-446655440000",
    "title": "Untagged Post",
    "content": "This post has no tags"
}'::jsonb);
-- Returns: status='created', cascade.created.tags=[]

-- Invalid author
SELECT * FROM create_blog_post_with_tags('{
    "author_id": "00000000-0000-0000-0000-000000000000",
    "title": "Test Post",
    "content": "Content"
}'::jsonb);
-- Returns: status='not_found:author'

-- ============================================================================
-- GraphQL Usage
-- ============================================================================

/*
mutation CreatePost {
  createBlogPostWithTags(input: {
    authorId: "550e8400-e29b-41d4-a716-446655440000"
    title: "Getting Started"
    content: "..."
    tags: ["graphql", "postgresql"]
  }) {
    ... on CreatePostSuccess {
      post {
        id
        title
        author { name }
      }
      cascade {
        created {
          tags {
            id
            name
            slug
          }
        }
      }
    }
  }
}
*/

-- ============================================================================
-- Advanced Pattern: Multiple Child Types
-- ============================================================================

CREATE OR REPLACE FUNCTION create_invoice_with_items(input_payload jsonb)
RETURNS mutation_response AS $$
DECLARE
    result mutation_response;
    invoice_record record;
    item jsonb;
    line_item_record record;
    created_items jsonb := '[]'::jsonb;
    customer_id uuid := (input_payload->>'customer_id')::uuid;
    items_data jsonb := input_payload->'items';
    total_amount numeric := 0;
BEGIN
    -- Create parent: Invoice
    INSERT INTO invoices (customer_id, status, total_amount)
    VALUES (customer_id, 'draft', 0)  -- Total calculated after items
    RETURNING * INTO invoice_record;

    -- Create children: Line items
    FOR item IN SELECT jsonb_array_elements(items_data)
    LOOP
        INSERT INTO invoice_items (
            invoice_id,
            description,
            quantity,
            unit_price,
            amount
        ) VALUES (
            invoice_record.id,
            item->>'description',
            (item->>'quantity')::int,
            (item->>'unit_price')::numeric,
            (item->>'quantity')::int * (item->>'unit_price')::numeric
        )
        RETURNING * INTO line_item_record;

        total_amount := total_amount + line_item_record.amount;
        created_items := created_items || to_jsonb(line_item_record);
    END LOOP;

    -- Update invoice total
    UPDATE invoices SET total_amount = total_amount
    WHERE id = invoice_record.id
    RETURNING * INTO invoice_record;

    result.status := 'created';
    result.message := format('Invoice created with %s item(s)', jsonb_array_length(created_items));
    result.entity := row_to_json(invoice_record);
    result.cascade := jsonb_build_object(
        'created', jsonb_build_object('items', created_items)
    );

    RETURN result;
END;
$$ LANGUAGE plpgsql;
