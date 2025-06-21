package com.fraiseql.benchmark.repository;

import com.fraiseql.benchmark.model.User;
import org.springframework.data.jpa.repository.JpaRepository;
import org.springframework.data.jpa.repository.Query;
import org.springframework.data.repository.query.Param;
import org.springframework.stereotype.Repository;

import java.util.List;
import java.util.Optional;

@Repository
public interface UserRepository extends JpaRepository<User, Integer> {
    Optional<User> findByEmail(String email);

    @Query("SELECT DISTINCT u FROM User u LEFT JOIN FETCH u.posts WHERE u.id IN :ids")
    List<User> findByIdsWithPosts(@Param("ids") List<Integer> ids);

    @Query("SELECT u FROM User u LEFT JOIN FETCH u.posts p LEFT JOIN FETCH p.comments WHERE u.id = :id")
    Optional<User> findByIdWithPostsAndComments(@Param("id") Integer id);
}
