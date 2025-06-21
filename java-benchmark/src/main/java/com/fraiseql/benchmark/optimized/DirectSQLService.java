package com.fraiseql.benchmark.optimized;

import com.fasterxml.jackson.databind.ObjectMapper;
import org.springframework.jdbc.core.JdbcTemplate;
import org.springframework.stereotype.Service;

import java.util.List;
import java.util.Map;

@Service
public class DirectSQLService {
    private final JdbcTemplate jdbcTemplate;
    private final ObjectMapper objectMapper;

    public DirectSQLService(JdbcTemplate jdbcTemplate, ObjectMapper objectMapper) {
        this.jdbcTemplate = jdbcTemplate;
        this.objectMapper = objectMapper;
    }

    public Map<String, Object> executeGraphQLQuery(String query, Map<String, Object> variables) {
        // This mimics FraiseQL's approach - direct SQL translation
        String sql = translateGraphQLToSQL(query, variables);
        return jdbcTemplate.queryForMap(sql);
    }

    public Map<String, Object> getUserWithPosts(Integer userId) {
        String sql = """
            SELECT jsonb_build_object(
                'id', u.id,
                'name', u.name,
                'email', u.email,
                'posts', COALESCE(
                    jsonb_agg(
                        jsonb_build_object(
                            'id', p.id,
                            'title', p.title,
                            'content', p.content,
                            'createdAt', p.created_at
                        ) ORDER BY p.created_at DESC
                    ) FILTER (WHERE p.id IS NOT NULL),
                    '[]'::jsonb
                )
            ) as result
            FROM users u
            LEFT JOIN posts p ON u.id = p.author_id
            WHERE u.id = ?
            GROUP BY u.id, u.name, u.email
        """;

        return jdbcTemplate.queryForMap(sql, userId);
    }

    public List<Map<String, Object>> getAllUsersWithPostCount() {
        String sql = """
            SELECT jsonb_build_object(
                'id', u.id,
                'name', u.name,
                'email', u.email,
                'postCount', COUNT(p.id)
            ) as result
            FROM users u
            LEFT JOIN posts p ON u.id = p.author_id
            GROUP BY u.id, u.name, u.email
            ORDER BY u.created_at DESC
        """;

        return jdbcTemplate.queryForList(sql);
    }

    public Map<String, Object> getPostWithCommentsAndAuthors(Integer postId) {
        String sql = """
            SELECT jsonb_build_object(
                'id', p.id,
                'title', p.title,
                'content', p.content,
                'author', jsonb_build_object(
                    'id', u.id,
                    'name', u.name,
                    'email', u.email
                ),
                'comments', COALESCE(
                    jsonb_agg(
                        jsonb_build_object(
                            'id', c.id,
                            'content', c.content,
                            'author', jsonb_build_object(
                                'id', cu.id,
                                'name', cu.name
                            )
                        ) ORDER BY c.created_at
                    ) FILTER (WHERE c.id IS NOT NULL),
                    '[]'::jsonb
                )
            ) as result
            FROM posts p
            JOIN users u ON p.author_id = u.id
            LEFT JOIN comments c ON p.id = c.post_id
            LEFT JOIN users cu ON c.author_id = cu.id
            WHERE p.id = ?
            GROUP BY p.id, p.title, p.content, u.id, u.name, u.email
        """;

        return jdbcTemplate.queryForMap(sql, postId);
    }

    private String translateGraphQLToSQL(String graphQLQuery, Map<String, Object> variables) {
        // Simplified translation logic - in real implementation would parse GraphQL AST
        // This is just for benchmark purposes
        if (graphQLQuery.contains("user") && graphQLQuery.contains("posts")) {
            return """
                SELECT jsonb_build_object(
                    'data', jsonb_build_object(
                        'user', (
                            SELECT jsonb_build_object(
                                'id', u.id,
                                'name', u.name,
                                'email', u.email,
                                'posts', COALESCE(
                                    jsonb_agg(
                                        jsonb_build_object(
                                            'id', p.id,
                                            'title', p.title,
                                            'content', p.content
                                        )
                                    ) FILTER (WHERE p.id IS NOT NULL),
                                    '[]'::jsonb
                                )
                            )
                            FROM users u
                            LEFT JOIN posts p ON u.id = p.author_id
                            WHERE u.id = ?
                            GROUP BY u.id
                        )
                    )
                ) as result
            """;
        }

        return "SELECT '{}'::jsonb as result";
    }
}
