package com.fraiseql.benchmark.optimized;

import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.*;

import java.util.Map;

@RestController
@RequestMapping("/optimized/graphql")
public class OptimizedGraphQLController {
    private final DirectSQLService directSQLService;

    public OptimizedGraphQLController(DirectSQLService directSQLService) {
        this.directSQLService = directSQLService;
    }

    @PostMapping
    public ResponseEntity<Map<String, Object>> graphql(@RequestBody Map<String, Object> request) {
        String query = (String) request.get("query");
        Map<String, Object> variables = (Map<String, Object>) request.get("variables");

        Map<String, Object> result = directSQLService.executeGraphQLQuery(query, variables);
        return ResponseEntity.ok(result);
    }

    @GetMapping("/user/{id}")
    public ResponseEntity<Map<String, Object>> getUserWithPosts(@PathVariable Integer id) {
        return ResponseEntity.ok(directSQLService.getUserWithPosts(id));
    }

    @GetMapping("/post/{id}")
    public ResponseEntity<Map<String, Object>> getPostWithComments(@PathVariable Integer id) {
        return ResponseEntity.ok(directSQLService.getPostWithCommentsAndAuthors(id));
    }
}
