package com.fraiseql.benchmark.service;

import com.fraiseql.benchmark.model.User;
import com.fraiseql.benchmark.model.Post;
import com.fraiseql.benchmark.repository.UserRepository;
import com.fraiseql.benchmark.repository.PostRepository;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;

import java.util.List;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.Executor;

@Service
@Transactional(readOnly = true)
public class UserService {
    private final UserRepository userRepository;
    private final PostRepository postRepository;
    private final Executor executor;

    public UserService(UserRepository userRepository, PostRepository postRepository, Executor executor) {
        this.userRepository = userRepository;
        this.postRepository = postRepository;
        this.executor = executor;
    }

    public User findById(Integer id) {
        return userRepository.findById(id)
            .orElseThrow(() -> new RuntimeException("User not found"));
    }

    public List<User> findAll() {
        return userRepository.findAll();
    }

    public CompletableFuture<List<Post>> loadPosts(User user) {
        return CompletableFuture.supplyAsync(() ->
            postRepository.findByAuthorIdWithAuthor(user.getId()), executor);
    }
}
