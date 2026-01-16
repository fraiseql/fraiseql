"""
VelocityBench Blogging App Schema Definitions

Canonical blogging application schema that can be expressed in all 5 supported languages:
- Python
- TypeScript
- Go
- Java
- PHP

This is the reference schema that all language generators must be able to express.
Schema includes:
- User type
- Post type (with nested author)
- Comment type (with nested author, post, parent_comment)
- Queries: ping, user, users, posts, post, comments, comment
- Mutations: updateUser, createPost, createComment
"""

import json
from typing import Any


def get_velocitybench_schema() -> dict[str, Any]:
    """
    Get the canonical VelocityBench blogging app schema.

    This schema represents:
    - 3 types: User, Post, Comment
    - 7 queries with various pagination and filtering
    - 3 mutations for CRUD operations
    - Nested relationships (Post.author, Comment.author, Comment.post, Comment.parent_comment)
    """
    return {
        "types": [
            {
                "name": "User",
                "description": "User type - queries tv_user view returning composed user objects",
                "fields": [
                    {"name": "id", "type": "ID", "nullable": False},
                    {"name": "username", "type": "String", "nullable": False},
                    {"name": "email", "type": "String", "nullable": False},
                    {"name": "firstName", "type": "String", "nullable": True},
                    {"name": "lastName", "type": "String", "nullable": True},
                    {"name": "bio", "type": "String", "nullable": True},
                    {"name": "avatarUrl", "type": "String", "nullable": True},
                    {"name": "isActive", "type": "Boolean", "nullable": False},
                    {"name": "createdAt", "type": "String", "nullable": False},
                    {"name": "updatedAt", "type": "String", "nullable": False},
                ]
            },
            {
                "name": "Post",
                "description": "Post type with pre-composed author object",
                "fields": [
                    {"name": "id", "type": "ID", "nullable": False},
                    {"name": "title", "type": "String", "nullable": False},
                    {"name": "content", "type": "String", "nullable": True},
                    {"name": "excerpt", "type": "String", "nullable": True},
                    {"name": "status", "type": "String", "nullable": False},
                    {"name": "publishedAt", "type": "String", "nullable": True},
                    {"name": "createdAt", "type": "String", "nullable": False},
                    {"name": "updatedAt", "type": "String", "nullable": False},
                    {"name": "author", "type": "User", "nullable": False},
                ]
            },
            {
                "name": "Comment",
                "description": "Comment type with nested author, post, and parent comment",
                "fields": [
                    {"name": "id", "type": "ID", "nullable": False},
                    {"name": "content", "type": "String", "nullable": False},
                    {"name": "isApproved", "type": "Boolean", "nullable": False},
                    {"name": "createdAt", "type": "String", "nullable": False},
                    {"name": "updatedAt", "type": "String", "nullable": False},
                    {"name": "author", "type": "User", "nullable": False},
                    {"name": "post", "type": "Post", "nullable": False},
                    {"name": "parentComment", "type": "Comment", "nullable": True},
                ]
            }
        ],
        "queries": [
            {
                "name": "ping",
                "description": "Simple ping query for throughput testing",
                "arguments": [],
                "return_type": "String",
                "return_list": False,
                "sql_source": "fn_ping"
            },
            {
                "name": "user",
                "description": "Get user by UUID id",
                "arguments": [
                    {"name": "id", "type": "ID", "nullable": False}
                ],
                "return_type": "User",
                "return_list": False,
                "sql_source": "v_user"
            },
            {
                "name": "users",
                "description": "Get users list with pagination",
                "arguments": [
                    {"name": "limit", "type": "Int", "default": 10},
                    {"name": "offset", "type": "Int", "default": 0}
                ],
                "return_type": "User",
                "return_list": True,
                "sql_source": "v_users"
            },
            {
                "name": "post",
                "description": "Get post by UUID id",
                "arguments": [
                    {"name": "id", "type": "ID", "nullable": False}
                ],
                "return_type": "Post",
                "return_list": False,
                "sql_source": "v_post"
            },
            {
                "name": "posts",
                "description": "Get posts list with pagination",
                "arguments": [
                    {"name": "limit", "type": "Int", "default": 10},
                    {"name": "offset", "type": "Int", "default": 0}
                ],
                "return_type": "Post",
                "return_list": True,
                "sql_source": "v_posts"
            },
            {
                "name": "comment",
                "description": "Get comment by UUID id",
                "arguments": [
                    {"name": "id", "type": "ID", "nullable": False}
                ],
                "return_type": "Comment",
                "return_list": False,
                "sql_source": "v_comment"
            },
            {
                "name": "comments",
                "description": "Get comments list with pagination",
                "arguments": [
                    {"name": "limit", "type": "Int", "default": 10},
                    {"name": "offset", "type": "Int", "default": 0}
                ],
                "return_type": "Comment",
                "return_list": True,
                "sql_source": "v_comments"
            }
        ],
        "mutations": [
            {
                "name": "updateUser",
                "description": "Update user information",
                "arguments": [
                    {"name": "id", "type": "ID", "nullable": False},
                    {"name": "firstName", "type": "String", "nullable": True},
                    {"name": "lastName", "type": "String", "nullable": True},
                    {"name": "bio", "type": "String", "nullable": True}
                ],
                "return_type": "User",
                "sql_source": "fn_update_user"
            },
            {
                "name": "createPost",
                "description": "Create a new post",
                "arguments": [
                    {"name": "title", "type": "String", "nullable": False},
                    {"name": "content", "type": "String", "nullable": True},
                    {"name": "excerpt", "type": "String", "nullable": True},
                    {"name": "status", "type": "String", "default": "published"}
                ],
                "return_type": "Post",
                "sql_source": "fn_create_post"
            },
            {
                "name": "createComment",
                "description": "Create a new comment on a post",
                "arguments": [
                    {"name": "postId", "type": "ID", "nullable": False},
                    {"name": "content", "type": "String", "nullable": False},
                    {"name": "parentCommentId", "type": "ID", "nullable": True}
                ],
                "return_type": "Comment",
                "sql_source": "fn_create_comment"
            }
        ]
    }


def get_python_schema_code() -> str:
    """Get Python code to define the VelocityBench schema."""
    return '''
from fraiseql import type as fraiseql_type, query as fraiseql_query, mutation as fraiseql_mutation, schema

@fraiseql_type
class User:
    """User type - queries tv_user view returning composed user objects"""
    id: str
    username: str
    email: str
    firstName: str | None = None
    lastName: str | None = None
    bio: str | None = None
    avatarUrl: str | None = None
    isActive: bool = True
    createdAt: str
    updatedAt: str


@fraiseql_type
class Post:
    """Post type with pre-composed author object"""
    id: str
    title: str
    content: str | None = None
    excerpt: str | None = None
    status: str = "published"
    publishedAt: str | None = None
    createdAt: str
    updatedAt: str
    author: User


@fraiseql_type
class Comment:
    """Comment type with nested author, post, and parent comment"""
    id: str
    content: str
    isApproved: bool = True
    createdAt: str
    updatedAt: str
    author: User
    post: Post
    parentComment: "Comment | None" = None


@fraiseql_query(sql_source="fn_ping")
def ping() -> str:
    """Simple ping query for throughput testing"""
    pass


@fraiseql_query(sql_source="v_user")
def user(id: str) -> User | None:
    """Get user by UUID id"""
    pass


@fraiseql_query(sql_source="v_users")
def users(limit: int = 10, offset: int = 0) -> list[User]:
    """Get users list with pagination"""
    pass


@fraiseql_query(sql_source="v_post")
def post(id: str) -> Post | None:
    """Get post by UUID id"""
    pass


@fraiseql_query(sql_source="v_posts")
def posts(limit: int = 10, offset: int = 0) -> list[Post]:
    """Get posts list with pagination"""
    pass


@fraiseql_query(sql_source="v_comment")
def comment(id: str) -> Comment | None:
    """Get comment by UUID id"""
    pass


@fraiseql_query(sql_source="v_comments")
def comments(limit: int = 10, offset: int = 0) -> list[Comment]:
    """Get comments list with pagination"""
    pass


@fraiseql_mutation(sql_source="fn_update_user")
def updateUser(id: str, firstName: str | None = None, lastName: str | None = None, bio: str | None = None) -> User:
    """Update user information"""
    pass


@fraiseql_mutation(sql_source="fn_create_post")
def createPost(title: str, content: str | None = None, excerpt: str | None = None, status: str = "published") -> Post:
    """Create a new post"""
    pass


@fraiseql_mutation(sql_source="fn_create_comment")
def createComment(postId: str, content: str, parentCommentId: str | None = None) -> Comment:
    """Create a new comment on a post"""
    pass


# Export schema
schema.export_schema("velocitybench_schema.json")
'''


def get_typescript_schema_code() -> str:
    """Get TypeScript code to define the VelocityBench schema."""
    return '''
import { Type, Query, Mutation } from "fraiseql";

@Type()
class User {
  id!: string;
  username!: string;
  email!: string;
  firstName?: string;
  lastName?: string;
  bio?: string;
  avatarUrl?: string;
  isActive: boolean = true;
  createdAt!: string;
  updatedAt!: string;
}

@Type()
class Post {
  id!: string;
  title!: string;
  content?: string;
  excerpt?: string;
  status: string = "published";
  publishedAt?: string;
  createdAt!: string;
  updatedAt!: string;
  author!: User;
}

@Type()
class Comment {
  id!: string;
  content!: string;
  isApproved: boolean = true;
  createdAt!: string;
  updatedAt!: string;
  author!: User;
  post!: Post;
  parentComment?: Comment;
}

@Query(sql_source = "fn_ping")
ping(): string {
  return "";
}

@Query(sql_source = "v_user")
user(id: string): User | null {
  return null;
}

@Query(sql_source = "v_users")
users(limit?: number, offset?: number): User[] {
  return [];
}

@Query(sql_source = "v_post")
post(id: string): Post | null {
  return null;
}

@Query(sql_source = "v_posts")
posts(limit?: number, offset?: number): Post[] {
  return [];
}

@Query(sql_source = "v_comment")
comment(id: string): Comment | null {
  return null;
}

@Query(sql_source = "v_comments")
comments(limit?: number, offset?: number): Comment[] {
  return [];
}

@Mutation(sql_source = "fn_update_user")
updateUser(id: string, firstName?: string, lastName?: string, bio?: string): User {
  return new User();
}

@Mutation(sql_source = "fn_create_post")
createPost(title: string, content?: string, excerpt?: string, status?: string): Post {
  return new Post();
}

@Mutation(sql_source = "fn_create_comment")
createComment(postId: string, content: string, parentCommentId?: string): Comment {
  return new Comment();
}
'''


def get_java_schema_code() -> str:
    """Get Java code to define the VelocityBench schema."""
    return '''\
import com.fraiseql.annotations.FraiseQLType;
import com.fraiseql.annotations.Query;
import com.fraiseql.annotations.Mutation;
import com.fraiseql.annotations.Field;
import java.util.List;

@FraiseQLType
public class User {
    @Field(nullable = false)
    private String id;

    @Field(nullable = false)
    private String username;

    @Field(nullable = false)
    private String email;

    @Field(nullable = true)
    private String firstName;

    @Field(nullable = true)
    private String lastName;

    @Field(nullable = true)
    private String bio;

    @Field(nullable = true)
    private String avatarUrl;

    @Field(nullable = false)
    private boolean isActive;

    @Field(nullable = false)
    private String createdAt;

    @Field(nullable = false)
    private String updatedAt;
}

@FraiseQLType
public class Post {
    @Field(nullable = false)
    private String id;

    @Field(nullable = false)
    private String title;

    @Field(nullable = true)
    private String content;

    @Field(nullable = true)
    private String excerpt;

    @Field(nullable = false)
    private String status;

    @Field(nullable = true)
    private String publishedAt;

    @Field(nullable = false)
    private String createdAt;

    @Field(nullable = false)
    private String updatedAt;

    @Field(nullable = false)
    private User author;
}

@FraiseQLType
public class Comment {
    @Field(nullable = false)
    private String id;

    @Field(nullable = false)
    private String content;

    @Field(nullable = false)
    private boolean isApproved;

    @Field(nullable = false)
    private String createdAt;

    @Field(nullable = false)
    private String updatedAt;

    @Field(nullable = false)
    private User author;

    @Field(nullable = false)
    private Post post;

    @Field(nullable = true)
    private Comment parentComment;
}

public class VelocityBenchSchema {
    @Query(sqlSource = "fn_ping")
    public String ping() {
        return null;
    }

    @Query(sqlSource = "v_user")
    public User user(String id) {
        return null;
    }

    @Query(sqlSource = "v_users")
    public List<User> users(Integer limit, Integer offset) {
        return null;
    }

    @Query(sqlSource = "v_post")
    public Post post(String id) {
        return null;
    }

    @Query(sqlSource = "v_posts")
    public List<Post> posts(Integer limit, Integer offset) {
        return null;
    }

    @Query(sqlSource = "v_comment")
    public Comment comment(String id) {
        return null;
    }

    @Query(sqlSource = "v_comments")
    public List<Comment> comments(Integer limit, Integer offset) {
        return null;
    }

    @Mutation(sqlSource = "fn_update_user")
    public User updateUser(String id, String firstName, String lastName, String bio) {
        return null;
    }

    @Mutation(sqlSource = "fn_create_post")
    public Post createPost(String title, String content, String excerpt, String status) {
        return null;
    }

    @Mutation(sqlSource = "fn_create_comment")
    public Comment createComment(String postId, String content, String parentCommentId) {
        return null;
    }
}
'''


def get_go_schema_code() -> str:
    """Get Go code to define the VelocityBench schema."""
    return '''\
package velocitybench

type User struct {
    ID        string `fraiseql:"id,required"`
    Username  string `fraiseql:"username,required"`
    Email     string `fraiseql:"email,required"`
    FirstName *string `fraiseql:"firstName"`
    LastName  *string `fraiseql:"lastName"`
    Bio       *string `fraiseql:"bio"`
    AvatarUrl *string `fraiseql:"avatarUrl"`
    IsActive  bool `fraiseql:"isActive,required"`
    CreatedAt string `fraiseql:"createdAt,required"`
    UpdatedAt string `fraiseql:"updatedAt,required"`
}

type Post struct {
    ID          string `fraiseql:"id,required"`
    Title       string `fraiseql:"title,required"`
    Content     *string `fraiseql:"content"`
    Excerpt     *string `fraiseql:"excerpt"`
    Status      string `fraiseql:"status,required"`
    PublishedAt *string `fraiseql:"publishedAt"`
    CreatedAt   string `fraiseql:"createdAt,required"`
    UpdatedAt   string `fraiseql:"updatedAt,required"`
    Author      User `fraiseql:"author,required"`
}

type Comment struct {
    ID            string `fraiseql:"id,required"`
    Content       string `fraiseql:"content,required"`
    IsApproved    bool `fraiseql:"isApproved,required"`
    CreatedAt     string `fraiseql:"createdAt,required"`
    UpdatedAt     string `fraiseql:"updatedAt,required"`
    Author        User `fraiseql:"author,required"`
    Post          Post `fraiseql:"post,required"`
    ParentComment *Comment `fraiseql:"parentComment"`
}

func (s *Schema) Ping() (string, error) {
    return "", nil
}

func (s *Schema) User(id string) (*User, error) {
    return nil, nil
}

func (s *Schema) Users(limit, offset int) ([]*User, error) {
    return nil, nil
}

func (s *Schema) Post(id string) (*Post, error) {
    return nil, nil
}

func (s *Schema) Posts(limit, offset int) ([]*Post, error) {
    return nil, nil
}

func (s *Schema) Comment(id string) (*Comment, error) {
    return nil, nil
}

func (s *Schema) Comments(limit, offset int) ([]*Comment, error) {
    return nil, nil
}

func (s *Schema) UpdateUser(id, firstName, lastName, bio string) (*User, error) {
    return nil, nil
}

func (s *Schema) CreatePost(title, content, excerpt, status string) (*Post, error) {
    return nil, nil
}

func (s *Schema) CreateComment(postId, content, parentCommentId string) (*Comment, error) {
    return nil, nil
}
'''


def get_php_schema_code() -> str:
    """Get PHP code to define the VelocityBench schema."""
    return '''\
<?php

namespace VelocityBench;

use FraiseQL\\Attributes\\Type;
use FraiseQL\\Attributes\\Field;
use FraiseQL\\Attributes\\Query;
use FraiseQL\\Attributes\\Mutation;

#[Type]
class User {
    #[Field(nullable: false)]
    public string $id;

    #[Field(nullable: false)]
    public string $username;

    #[Field(nullable: false)]
    public string $email;

    #[Field(nullable: true)]
    public ?string $firstName;

    #[Field(nullable: true)]
    public ?string $lastName;

    #[Field(nullable: true)]
    public ?string $bio;

    #[Field(nullable: true)]
    public ?string $avatarUrl;

    #[Field(nullable: false)]
    public bool $isActive;

    #[Field(nullable: false)]
    public string $createdAt;

    #[Field(nullable: false)]
    public string $updatedAt;
}

#[Type]
class Post {
    #[Field(nullable: false)]
    public string $id;

    #[Field(nullable: false)]
    public string $title;

    #[Field(nullable: true)]
    public ?string $content;

    #[Field(nullable: true)]
    public ?string $excerpt;

    #[Field(nullable: false)]
    public string $status;

    #[Field(nullable: true)]
    public ?string $publishedAt;

    #[Field(nullable: false)]
    public string $createdAt;

    #[Field(nullable: false)]
    public string $updatedAt;

    #[Field(nullable: false)]
    public User $author;
}

#[Type]
class Comment {
    #[Field(nullable: false)]
    public string $id;

    #[Field(nullable: false)]
    public string $content;

    #[Field(nullable: false)]
    public bool $isApproved;

    #[Field(nullable: false)]
    public string $createdAt;

    #[Field(nullable: false)]
    public string $updatedAt;

    #[Field(nullable: false)]
    public User $author;

    #[Field(nullable: false)]
    public Post $post;

    #[Field(nullable: true)]
    public ?Comment $parentComment;
}

class VelocityBenchSchema {
    #[Query(sqlSource: "fn_ping")]
    public function ping(): string {
        return "";
    }

    #[Query(sqlSource: "v_user")]
    public function user(string $id): ?User {
        return null;
    }

    #[Query(sqlSource: "v_users")]
    public function users(int $limit = 10, int $offset = 0): array {
        return [];
    }

    #[Query(sqlSource: "v_post")]
    public function post(string $id): ?Post {
        return null;
    }

    #[Query(sqlSource: "v_posts")]
    public function posts(int $limit = 10, int $offset = 0): array {
        return [];
    }

    #[Query(sqlSource: "v_comment")]
    public function comment(string $id): ?Comment {
        return null;
    }

    #[Query(sqlSource: "v_comments")]
    public function comments(int $limit = 10, int $offset = 0): array {
        return [];
    }

    #[Mutation(sqlSource: "fn_update_user")]
    public function updateUser(string $id, ?string $firstName = null, ?string $lastName = null, ?string $bio = null): User {
        return new User();
    }

    #[Mutation(sqlSource: "fn_create_post")]
    public function createPost(string $title, ?string $content = null, ?string $excerpt = null, string $status = "published"): Post {
        return new Post();
    }

    #[Mutation(sqlSource: "fn_create_comment")]
    public function createComment(string $postId, string $content, ?string $parentCommentId = null): Comment {
        return new Comment();
    }
}
'''


if __name__ == "__main__":
    schema = get_velocitybench_schema()
    print("VelocityBench Blogging App Schema:")
    print(json.dumps(schema, indent=2))
