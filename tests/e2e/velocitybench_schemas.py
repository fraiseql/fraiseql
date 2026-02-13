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
                "returns_list": False,
                "sql_source": "fn_ping"
            },
            {
                "name": "user",
                "description": "Get user by UUID id",
                "arguments": [
                    {"name": "id", "type": "ID", "nullable": False}
                ],
                "return_type": "User",
                "returns_list": False,
                "sql_source": "v_user"
            },
            {
                "name": "users",
                "description": "Get users list with pagination",
                "arguments": [
                    {"name": "limit", "type": "Int", "nullable": True, "default": 10},
                    {"name": "offset", "type": "Int", "nullable": True, "default": 0}
                ],
                "return_type": "User",
                "returns_list": True,
                "sql_source": "v_users"
            },
            {
                "name": "post",
                "description": "Get post by UUID id",
                "arguments": [
                    {"name": "id", "type": "ID", "nullable": False}
                ],
                "return_type": "Post",
                "returns_list": False,
                "sql_source": "v_post"
            },
            {
                "name": "posts",
                "description": "Get posts list with pagination",
                "arguments": [
                    {"name": "limit", "type": "Int", "nullable": True, "default": 10},
                    {"name": "offset", "type": "Int", "nullable": True, "default": 0}
                ],
                "return_type": "Post",
                "returns_list": True,
                "sql_source": "v_posts"
            },
            {
                "name": "comment",
                "description": "Get comment by UUID id",
                "arguments": [
                    {"name": "id", "type": "ID", "nullable": False}
                ],
                "return_type": "Comment",
                "returns_list": False,
                "sql_source": "v_comment"
            },
            {
                "name": "comments",
                "description": "Get comments list with pagination",
                "arguments": [
                    {"name": "limit", "type": "Int", "nullable": True, "default": 10},
                    {"name": "offset", "type": "Int", "nullable": True, "default": 0}
                ],
                "return_type": "Comment",
                "returns_list": True,
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
                "returns_list": False,
                "nullable": False,
                "sql_source": "fn_update_user"
            },
            {
                "name": "createPost",
                "description": "Create a new post",
                "arguments": [
                    {"name": "title", "type": "String", "nullable": False},
                    {"name": "content", "type": "String", "nullable": True},
                    {"name": "excerpt", "type": "String", "nullable": True},
                    {"name": "status", "type": "String", "nullable": True, "default": "published"}
                ],
                "return_type": "Post",
                "returns_list": False,
                "nullable": False,
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
                "returns_list": False,
                "nullable": False,
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


def get_kotlin_schema_code() -> str:
    """Get Kotlin code to define the VelocityBench schema."""
    return '''\
import com.fraiseql.annotations.FraiseQLType
import com.fraiseql.annotations.Query
import com.fraiseql.annotations.Mutation
import com.fraiseql.annotations.Field

@FraiseQLType
data class User(
    @Field(nullable = false)
    val id: String,

    @Field(nullable = false)
    val username: String,

    @Field(nullable = false)
    val email: String,

    @Field(nullable = true)
    val firstName: String? = null,

    @Field(nullable = true)
    val lastName: String? = null,

    @Field(nullable = true)
    val bio: String? = null,

    @Field(nullable = true)
    val avatarUrl: String? = null,

    @Field(nullable = false)
    val isActive: Boolean = true,

    @Field(nullable = false)
    val createdAt: String,

    @Field(nullable = false)
    val updatedAt: String
)

@FraiseQLType
data class Post(
    @Field(nullable = false)
    val id: String,

    @Field(nullable = false)
    val title: String,

    @Field(nullable = true)
    val content: String? = null,

    @Field(nullable = true)
    val excerpt: String? = null,

    @Field(nullable = false)
    val status: String = "published",

    @Field(nullable = true)
    val publishedAt: String? = null,

    @Field(nullable = false)
    val createdAt: String,

    @Field(nullable = false)
    val updatedAt: String,

    @Field(nullable = false)
    val author: User
)

@FraiseQLType
data class Comment(
    @Field(nullable = false)
    val id: String,

    @Field(nullable = false)
    val content: String,

    @Field(nullable = false)
    val isApproved: Boolean = true,

    @Field(nullable = false)
    val createdAt: String,

    @Field(nullable = false)
    val updatedAt: String,

    @Field(nullable = false)
    val author: User,

    @Field(nullable = false)
    val post: Post,

    @Field(nullable = true)
    val parentComment: Comment? = null
)

class VelocityBenchSchema {
    @Query(sqlSource = "fn_ping")
    fun ping(): String {
        return ""
    }

    @Query(sqlSource = "v_user")
    fun user(id: String): User? {
        return null
    }

    @Query(sqlSource = "v_users")
    fun users(limit: Int = 10, offset: Int = 0): List<User> {
        return emptyList()
    }

    @Query(sqlSource = "v_post")
    fun post(id: String): Post? {
        return null
    }

    @Query(sqlSource = "v_posts")
    fun posts(limit: Int = 10, offset: Int = 0): List<Post> {
        return emptyList()
    }

    @Query(sqlSource = "v_comment")
    fun comment(id: String): Comment? {
        return null
    }

    @Query(sqlSource = "v_comments")
    fun comments(limit: Int = 10, offset: Int = 0): List<Comment> {
        return emptyList()
    }

    @Mutation(sqlSource = "fn_update_user")
    fun updateUser(id: String, firstName: String? = null, lastName: String? = null, bio: String? = null): User {
        return User(id = id, username = "", email = "", createdAt = "", updatedAt = "")
    }

    @Mutation(sqlSource = "fn_create_post")
    fun createPost(title: String, content: String? = null, excerpt: String? = null, status: String = "published"): Post {
        return Post(id = "", title = title, createdAt = "", updatedAt = "", author = User(id = "", username = "", email = "", createdAt = "", updatedAt = ""))
    }

    @Mutation(sqlSource = "fn_create_comment")
    fun createComment(postId: String, content: String, parentCommentId: String? = null): Comment {
        return Comment(id = "", content = content, createdAt = "", updatedAt = "", author = User(id = "", username = "", email = "", createdAt = "", updatedAt = ""), post = Post(id = "", title = "", createdAt = "", updatedAt = "", author = User(id = "", username = "", email = "", createdAt = "", updatedAt = "")))
    }
}
'''


def get_csharp_schema_code() -> str:
    """Get C# code to define the VelocityBench schema."""
    return '''\
using FraiseQL.Annotations;
using System;
using System.Collections.Generic;

[FraiseQLType]
public record User(
    [Field(Nullable = false)]
    string Id,

    [Field(Nullable = false)]
    string Username,

    [Field(Nullable = false)]
    string Email,

    [Field(Nullable = true)]
    string? FirstName = null,

    [Field(Nullable = true)]
    string? LastName = null,

    [Field(Nullable = true)]
    string? Bio = null,

    [Field(Nullable = true)]
    string? AvatarUrl = null,

    [Field(Nullable = false)]
    bool IsActive = true,

    [Field(Nullable = false)]
    string CreatedAt = "",

    [Field(Nullable = false)]
    string UpdatedAt = ""
);

[FraiseQLType]
public record Post(
    [Field(Nullable = false)]
    string Id,

    [Field(Nullable = false)]
    string Title,

    [Field(Nullable = true)]
    string? Content = null,

    [Field(Nullable = true)]
    string? Excerpt = null,

    [Field(Nullable = false)]
    string Status = "published",

    [Field(Nullable = true)]
    string? PublishedAt = null,

    [Field(Nullable = false)]
    string CreatedAt,

    [Field(Nullable = false)]
    string UpdatedAt,

    [Field(Nullable = false)]
    User Author
);

[FraiseQLType]
public record Comment(
    [Field(Nullable = false)]
    string Id,

    [Field(Nullable = false)]
    string Content,

    [Field(Nullable = false)]
    bool IsApproved = true,

    [Field(Nullable = false)]
    string CreatedAt,

    [Field(Nullable = false)]
    string UpdatedAt,

    [Field(Nullable = false)]
    User Author,

    [Field(Nullable = false)]
    Post Post,

    [Field(Nullable = true)]
    Comment? ParentComment = null
);

public class VelocityBenchSchema
{
    [Query(SqlSource = "fn_ping")]
    public string Ping()
    {
        return "";
    }

    [Query(SqlSource = "v_user")]
    public User? User(string id)
    {
        return null;
    }

    [Query(SqlSource = "v_users")]
    public List<User> Users(int limit = 10, int offset = 0)
    {
        return new List<User>();
    }

    [Query(SqlSource = "v_post")]
    public Post? Post(string id)
    {
        return null;
    }

    [Query(SqlSource = "v_posts")]
    public List<Post> Posts(int limit = 10, int offset = 0)
    {
        return new List<Post>();
    }

    [Query(SqlSource = "v_comment")]
    public Comment? Comment(string id)
    {
        return null;
    }

    [Query(SqlSource = "v_comments")]
    public List<Comment> Comments(int limit = 10, int offset = 0)
    {
        return new List<Comment>();
    }

    [Mutation(SqlSource = "fn_update_user")]
    public User UpdateUser(string id, string? firstName = null, string? lastName = null, string? bio = null)
    {
        return new User(id, "", "", firstName, lastName, bio);
    }

    [Mutation(SqlSource = "fn_create_post")]
    public Post CreatePost(string title, string? content = null, string? excerpt = null, string status = "published")
    {
        return new Post("", title, content, excerpt, status, null, "", "", null!);
    }

    [Mutation(SqlSource = "fn_create_comment")]
    public Comment CreateComment(string postId, string content, string? parentCommentId = null)
    {
        return new Comment("", content, true, "", "", null!, null!, null);
    }
}
'''


def get_rust_schema_code() -> str:
    """Get Rust code to define the VelocityBench schema."""
    return '''\
use fraiseql::prelude::*;

#[derive(FraiseQLType, Debug, Clone)]
pub struct User {
    #[field(nullable = false)]
    pub id: String,

    #[field(nullable = false)]
    pub username: String,

    #[field(nullable = false)]
    pub email: String,

    #[field(nullable = true)]
    pub first_name: Option<String>,

    #[field(nullable = true)]
    pub last_name: Option<String>,

    #[field(nullable = true)]
    pub bio: Option<String>,

    #[field(nullable = true)]
    pub avatar_url: Option<String>,

    #[field(nullable = false)]
    pub is_active: bool,

    #[field(nullable = false)]
    pub created_at: String,

    #[field(nullable = false)]
    pub updated_at: String,
}

#[derive(FraiseQLType, Debug, Clone)]
pub struct Post {
    #[field(nullable = false)]
    pub id: String,

    #[field(nullable = false)]
    pub title: String,

    #[field(nullable = true)]
    pub content: Option<String>,

    #[field(nullable = true)]
    pub excerpt: Option<String>,

    #[field(nullable = false)]
    pub status: String,

    #[field(nullable = true)]
    pub published_at: Option<String>,

    #[field(nullable = false)]
    pub created_at: String,

    #[field(nullable = false)]
    pub updated_at: String,

    #[field(nullable = false)]
    pub author: User,
}

#[derive(FraiseQLType, Debug, Clone)]
pub struct Comment {
    #[field(nullable = false)]
    pub id: String,

    #[field(nullable = false)]
    pub content: String,

    #[field(nullable = false)]
    pub is_approved: bool,

    #[field(nullable = false)]
    pub created_at: String,

    #[field(nullable = false)]
    pub updated_at: String,

    #[field(nullable = false)]
    pub author: User,

    #[field(nullable = false)]
    pub post: Post,

    #[field(nullable = true)]
    pub parent_comment: Option<Box<Comment>>,
}

pub struct VelocityBenchSchema;

impl VelocityBenchSchema {
    #[query(sql_source = "fn_ping")]
    pub fn ping() -> String {
        String::new()
    }

    #[query(sql_source = "v_user")]
    pub fn user(id: String) -> Option<User> {
        None
    }

    #[query(sql_source = "v_users")]
    pub fn users(limit: Option<i32>, offset: Option<i32>) -> Vec<User> {
        vec![]
    }

    #[query(sql_source = "v_post")]
    pub fn post(id: String) -> Option<Post> {
        None
    }

    #[query(sql_source = "v_posts")]
    pub fn posts(limit: Option<i32>, offset: Option<i32>) -> Vec<Post> {
        vec![]
    }

    #[query(sql_source = "v_comment")]
    pub fn comment(id: String) -> Option<Comment> {
        None
    }

    #[query(sql_source = "v_comments")]
    pub fn comments(limit: Option<i32>, offset: Option<i32>) -> Vec<Comment> {
        vec![]
    }

    #[mutation(sql_source = "fn_update_user")]
    pub fn update_user(id: String, first_name: Option<String>, last_name: Option<String>, bio: Option<String>) -> User {
        User {
            id,
            username: String::new(),
            email: String::new(),
            first_name,
            last_name,
            bio,
            avatar_url: None,
            is_active: true,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[mutation(sql_source = "fn_create_post")]
    pub fn create_post(title: String, content: Option<String>, excerpt: Option<String>, status: Option<String>) -> Post {
        Post {
            id: String::new(),
            title,
            content,
            excerpt,
            status: status.unwrap_or_else(|| "published".to_string()),
            published_at: None,
            created_at: String::new(),
            updated_at: String::new(),
            author: User {
                id: String::new(),
                username: String::new(),
                email: String::new(),
                first_name: None,
                last_name: None,
                bio: None,
                avatar_url: None,
                is_active: true,
                created_at: String::new(),
                updated_at: String::new(),
            },
        }
    }

    #[mutation(sql_source = "fn_create_comment")]
    pub fn create_comment(post_id: String, content: String, parent_comment_id: Option<String>) -> Comment {
        Comment {
            id: String::new(),
            content,
            is_approved: true,
            created_at: String::new(),
            updated_at: String::new(),
            author: User {
                id: String::new(),
                username: String::new(),
                email: String::new(),
                first_name: None,
                last_name: None,
                bio: None,
                avatar_url: None,
                is_active: true,
                created_at: String::new(),
                updated_at: String::new(),
            },
            post: Post {
                id: String::new(),
                title: String::new(),
                content: None,
                excerpt: None,
                status: "published".to_string(),
                published_at: None,
                created_at: String::new(),
                updated_at: String::new(),
                author: User {
                    id: String::new(),
                    username: String::new(),
                    email: String::new(),
                    first_name: None,
                    last_name: None,
                    bio: None,
                    avatar_url: None,
                    is_active: true,
                    created_at: String::new(),
                    updated_at: String::new(),
                },
            },
            parent_comment: None,
        }
    }
}
'''


def get_javascript_schema_code() -> str:
    """Get JavaScript code to define the VelocityBench schema."""
    return '''\
import { Type, Query, Mutation, Field } from "fraiseql";

@Type()
class User {
  @Field({ nullable: false })
  id;

  @Field({ nullable: false })
  username;

  @Field({ nullable: false })
  email;

  @Field({ nullable: true })
  firstName;

  @Field({ nullable: true })
  lastName;

  @Field({ nullable: true })
  bio;

  @Field({ nullable: true })
  avatarUrl;

  @Field({ nullable: false })
  isActive = true;

  @Field({ nullable: false })
  createdAt;

  @Field({ nullable: false })
  updatedAt;
}

@Type()
class Post {
  @Field({ nullable: false })
  id;

  @Field({ nullable: false })
  title;

  @Field({ nullable: true })
  content;

  @Field({ nullable: true })
  excerpt;

  @Field({ nullable: false })
  status = "published";

  @Field({ nullable: true })
  publishedAt;

  @Field({ nullable: false })
  createdAt;

  @Field({ nullable: false })
  updatedAt;

  @Field({ nullable: false })
  author;
}

@Type()
class Comment {
  @Field({ nullable: false })
  id;

  @Field({ nullable: false })
  content;

  @Field({ nullable: false })
  isApproved = true;

  @Field({ nullable: false })
  createdAt;

  @Field({ nullable: false })
  updatedAt;

  @Field({ nullable: false })
  author;

  @Field({ nullable: false })
  post;

  @Field({ nullable: true })
  parentComment;
}

class VelocityBenchSchema {
  @Query({ sqlSource: "fn_ping" })
  ping() {
    return "";
  }

  @Query({ sqlSource: "v_user" })
  user(id) {
    return null;
  }

  @Query({ sqlSource: "v_users" })
  users(limit = 10, offset = 0) {
    return [];
  }

  @Query({ sqlSource: "v_post" })
  post(id) {
    return null;
  }

  @Query({ sqlSource: "v_posts" })
  posts(limit = 10, offset = 0) {
    return [];
  }

  @Query({ sqlSource: "v_comment" })
  comment(id) {
    return null;
  }

  @Query({ sqlSource: "v_comments" })
  comments(limit = 10, offset = 0) {
    return [];
  }

  @Mutation({ sqlSource: "fn_update_user" })
  updateUser(id, firstName, lastName, bio) {
    return new User();
  }

  @Mutation({ sqlSource: "fn_create_post" })
  createPost(title, content, excerpt, status = "published") {
    return new Post();
  }

  @Mutation({ sqlSource: "fn_create_comment" })
  createComment(postId, content, parentCommentId) {
    return new Comment();
  }
}

export { VelocityBenchSchema, User, Post, Comment };
'''


def get_ruby_schema_code() -> str:
    """Get Ruby code to define the VelocityBench schema."""
    return '''\
require "fraiseql"

class User
  include FraiseQL::Type

  fraiseql_field :id, :string, required: true
  fraiseql_field :username, :string, required: true
  fraiseql_field :email, :string, required: true
  fraiseql_field :first_name, :string, required: false
  fraiseql_field :last_name, :string, required: false
  fraiseql_field :bio, :string, required: false
  fraiseql_field :avatar_url, :string, required: false
  fraiseql_field :is_active, :boolean, required: true, default: true
  fraiseql_field :created_at, :string, required: true
  fraiseql_field :updated_at, :string, required: true

  attr_accessor :id, :username, :email, :first_name, :last_name, :bio, :avatar_url, :is_active, :created_at, :updated_at
end

class Post
  include FraiseQL::Type

  fraiseql_field :id, :string, required: true
  fraiseql_field :title, :string, required: true
  fraiseql_field :content, :string, required: false
  fraiseql_field :excerpt, :string, required: false
  fraiseql_field :status, :string, required: true, default: "published"
  fraiseql_field :published_at, :string, required: false
  fraiseql_field :created_at, :string, required: true
  fraiseql_field :updated_at, :string, required: true
  fraiseql_field :author, User, required: true

  attr_accessor :id, :title, :content, :excerpt, :status, :published_at, :created_at, :updated_at, :author
end

class Comment
  include FraiseQL::Type

  fraiseql_field :id, :string, required: true
  fraiseql_field :content, :string, required: true
  fraiseql_field :is_approved, :boolean, required: true, default: true
  fraiseql_field :created_at, :string, required: true
  fraiseql_field :updated_at, :string, required: true
  fraiseql_field :author, User, required: true
  fraiseql_field :post, Post, required: true
  fraiseql_field :parent_comment, Comment, required: false

  attr_accessor :id, :content, :is_approved, :created_at, :updated_at, :author, :post, :parent_comment
end

module VelocityBenchSchema
  extend FraiseQL::Schema

  fraiseql_query :ping, return_type: :string, sql_source: "fn_ping" do
    ""
  end

  fraiseql_query :user, arguments: { id: :string }, return_type: User, sql_source: "v_user" do |id|
    nil
  end

  fraiseql_query :users, arguments: { limit: { type: :integer, default: 10 }, offset: { type: :integer, default: 0 } }, return_type: [User], sql_source: "v_users" do |limit, offset|
    []
  end

  fraiseql_query :post, arguments: { id: :string }, return_type: Post, sql_source: "v_post" do |id|
    nil
  end

  fraiseql_query :posts, arguments: { limit: { type: :integer, default: 10 }, offset: { type: :integer, default: 0 } }, return_type: [Post], sql_source: "v_posts" do |limit, offset|
    []
  end

  fraiseql_query :comment, arguments: { id: :string }, return_type: Comment, sql_source: "v_comment" do |id|
    nil
  end

  fraiseql_query :comments, arguments: { limit: { type: :integer, default: 10 }, offset: { type: :integer, default: 0 } }, return_type: [Comment], sql_source: "v_comments" do |limit, offset|
    []
  end

  fraiseql_mutation :update_user, arguments: { id: :string, first_name: { type: :string, required: false }, last_name: { type: :string, required: false }, bio: { type: :string, required: false } }, return_type: User, sql_source: "fn_update_user" do |id, first_name, last_name, bio|
    User.new
  end

  fraiseql_mutation :create_post, arguments: { title: :string, content: { type: :string, required: false }, excerpt: { type: :string, required: false }, status: { type: :string, default: "published" } }, return_type: Post, sql_source: "fn_create_post" do |title, content, excerpt, status|
    Post.new
  end

  fraiseql_mutation :create_comment, arguments: { post_id: :string, content: :string, parent_comment_id: { type: :string, required: false } }, return_type: Comment, sql_source: "fn_create_comment" do |post_id, content, parent_comment_id|
    Comment.new
  end
end
'''


if __name__ == "__main__":
    schema = get_velocitybench_schema()
    print("VelocityBench Blogging App Schema:")
    print(json.dumps(schema, indent=2))
