(defproject com.fraiseql/fraiseql-clojure "1.0.0"
  :description "Clojure authoring language for FraiseQL with 100% feature parity"
  :url "https://github.com/fraiseql/fraiseql"
  :license {:name "Apache License 2.0"
            :url "https://www.apache.org/licenses/LICENSE-2.0.txt"}
  :scm {:name "git"
        :url "https://github.com/fraiseql/fraiseql"}
  :repositories [["central" {:url "https://repo1.maven.org/maven2/"
                            :snapshots false}]
                 ["clojars" {:url "https://clojars.org/repo/"}]]
  :dependencies [[org.clojure/clojure "1.11.1"]]
  :plugins [[lein-cljfmt "0.9.2"]]
  :profiles {:dev {:dependencies [[org.clojure/clojure "1.11.1"]]}
             :test {:dependencies [[org.clojure/clojure "1.11.1"]]}})
