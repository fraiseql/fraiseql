#!/usr/bin/env python3
"""Script to fix camelCase issues in tests."""

import os
import re
from pathlib import Path

# Common field name conversions
FIELD_CONVERSIONS = {
    "get_user": "getUser",
    "get_users": "getUsers", 
    "list_users": "listUsers",
    "get_post": "getPost",
    "get_posts": "getPosts",
    "list_posts": "listPosts",
    "create_user": "createUser",
    "update_user": "updateUser",
    "delete_user": "deleteUser",
    "create_post": "createPost",
    "update_post": "updatePost",
    "delete_post": "deletePost",
    "create_item": "createItem",
    "get_item": "getItem",
    "author_id": "authorId",
    "user_id": "userId",
    "post_id": "postId",
    "created_at": "createdAt",
    "updated_at": "updatedAt",
    "is_active": "isActive",
    "is_published": "isPublished",
    "email_address": "emailAddress",
    "phone_number": "phoneNumber",
    "first_name": "firstName",
    "last_name": "lastName",
    "full_name": "fullName",
    "api_version": "apiVersion",
    "post_count": "postCount",
    "user_count": "userCount",
    "total_count": "totalCount",
    "page_size": "pageSize",
    "page_number": "pageNumber",
    "has_next": "hasNext",
    "has_previous": "hasPrevious",
    "get_posts_no_dataloader": "getPostsNoDataloader",
    "get_posts_with_dataloader": "getPostsWithDataloader",
    "custom_value": "customValue",
    "custom_resource": "customResource",
    "test_loader_query": "testLoaderQuery",
    "get_loader_test": "getLoaderTest",
}

def fix_graphql_queries(content):
    """Fix GraphQL query strings to use camelCase."""
    # Pattern to find GraphQL query/mutation strings
    graphql_pattern = r'(query|mutation)\s*(?:\w+)?\s*(?:\([^)]*\))?\s*\{[^}]+\}'
    
    def replace_fields(match):
        query = match.group(0)
        for snake, camel in FIELD_CONVERSIONS.items():
            # Replace in field names (not in arguments)
            query = re.sub(rf'\b{snake}\b(?=\s*[(\{{])', camel, query)
            # Replace in response field access
            query = re.sub(rf'\b{snake}\b(?=\s*:)', camel, query)
        return query
    
    # Fix inline GraphQL strings
    content = re.sub(graphql_pattern, replace_fields, content, flags=re.DOTALL)
    
    # Fix multiline GraphQL strings
    multiline_pattern = r'"""[\s\S]*?"""'
    
    def replace_in_multiline(match):
        query = match.group(0)
        for snake, camel in FIELD_CONVERSIONS.items():
            query = re.sub(rf'\b{snake}\b(?=\s*[(\{{])', camel, query)
            query = re.sub(rf'\b{snake}\b(?=\s*:)', camel, query)
        return query
    
    content = re.sub(multiline_pattern, replace_in_multiline, content)
    
    return content

def fix_response_access(content):
    """Fix response data access to use camelCase."""
    for snake, camel in FIELD_CONVERSIONS.items():
        # Fix dictionary access patterns
        content = re.sub(rf'\["data"\]\["{snake}"\]', f'["data"]["{camel}"]', content)
        content = re.sub(rf"data\['data'\]\['{snake}'\]", f"data['data']['{camel}']", content)
        
        # Fix in assertions
        content = re.sub(rf'"{snake}" in field_names', f'"{camel}" in field_names', content)
        content = re.sub(rf"'{snake}' in field_names", f"'{camel}' in field_names", content)
        
        # Fix in response data checks
        content = re.sub(rf'"{snake}" in response\.json\(\)\["data"\]', f'"{camel}" in response.json()["data"]', content)
        content = re.sub(rf"'{snake}' in response\.json\(\)\['data'\]", f"'{camel}' in response.json()['data']", content)
        
    return content

def fix_json_queries(content):
    """Fix JSON query patterns."""
    for snake, camel in FIELD_CONVERSIONS.items():
        # Fix in json= parameters
        content = re.sub(rf'"query":\s*"[^"]*\b{snake}\b', lambda m: m.group(0).replace(snake, camel), content)
        content = re.sub(rf'"query":\s*\'[^\']*\b{snake}\b', lambda m: m.group(0).replace(snake, camel), content)
    
    return content

def process_file(filepath):
    """Process a single test file."""
    with open(filepath, 'r') as f:
        content = f.read()
    
    original_content = content
    
    # Apply fixes
    content = fix_graphql_queries(content)
    content = fix_response_access(content)
    content = fix_json_queries(content)
    
    # Only write if changed
    if content != original_content:
        with open(filepath, 'w') as f:
            f.write(content)
        print(f"Fixed: {filepath}")
        return True
    return False

def main():
    """Main function to process all test files."""
    test_dir = Path("/home/lionel/code/fraiseql/tests")
    fixed_count = 0
    
    for test_file in test_dir.rglob("test_*.py"):
        if process_file(test_file):
            fixed_count += 1
    
    print(f"\nFixed {fixed_count} files")

if __name__ == "__main__":
    main()