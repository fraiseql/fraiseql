package com.fraiseql.benchmark.graphql;

import com.fraiseql.benchmark.model.User;
import com.fraiseql.benchmark.model.Post;
import com.fraiseql.benchmark.service.UserService;
import org.springframework.graphql.data.method.annotation.Argument;
import org.springframework.graphql.data.method.annotation.QueryMapping;
import org.springframework.graphql.data.method.annotation.SchemaMapping;
import org.springframework.stereotype.Controller;

import java.util.List;
import java.util.concurrent.CompletableFuture;

@Controller
public class UserResolver {
    private final UserService userService;

    public UserResolver(UserService userService) {
        this.userService = userService;
    }

    @QueryMapping
    public User user(@Argument Integer id) {
        return userService.findById(id);
    }

    @QueryMapping
    public List<User> users() {
        return userService.findAll();
    }

    @SchemaMapping(typeName = "User", field = "posts")
    public CompletableFuture<List<Post>> posts(User user) {
        return userService.loadPosts(user);
    }
}
