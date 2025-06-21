package com.fraiseql.benchmark.repository;

import com.fraiseql.benchmark.model.Post;
import org.springframework.data.jpa.repository.JpaRepository;
import org.springframework.data.jpa.repository.Query;
import org.springframework.data.repository.query.Param;
import org.springframework.stereotype.Repository;

import java.util.List;

@Repository
public interface PostRepository extends JpaRepository<Post, Integer> {
    @Query("SELECT p FROM Post p JOIN FETCH p.author WHERE p.author.id = :userId")
    List<Post> findByAuthorIdWithAuthor(@Param("userId") Integer userId);

    @Query("SELECT p FROM Post p LEFT JOIN FETCH p.comments c LEFT JOIN FETCH c.author WHERE p.id IN :ids")
    List<Post> findByIdsWithCommentsAndAuthors(@Param("ids") List<Integer> ids);
}
