ThisBuild / name := "fraiseql-scala"
ThisBuild / version := "1.0.0"
ThisBuild / organization := "com.fraiseql"
ThisBuild / organizationName := "FraiseQL Contributors"
ThisBuild / scalaVersion := "2.13.12"

lazy val root = (project in file("."))
  .settings(
    name := "fraiseql-scala",
    description := "Scala authoring language for FraiseQL with 100% feature parity",
    homepage := Some(url("https://github.com/fraiseql/fraiseql")),
    licenses := List("Apache-2.0" -> url("https://www.apache.org/licenses/LICENSE-2.0.txt")),
    developers := List(
      Developer(
        id = "fraiseql",
        name = "FraiseQL Contributors",
        email = "",
        url = url("https://github.com/fraiseql")
      )
    ),
    libraryDependencies ++= Seq(
      "org.scalatest" %% "scalatest" % "3.2.17" % Test
    ),
    scalacOptions ++= Seq(
      "-deprecation",
      "-feature",
      "-unchecked",
      "-Xfatal-warnings"
    ),
    Test / testOptions += Tests.Argument(TestFrameworks.ScalaTest, "-oDF")
  )
