plugins {
    kotlin("jvm") version "1.9.20"
    id("java-library")
    id("maven-publish")
}

group = "com.fraiseql"
version = "1.0.0"

repositories {
    mavenCentral()
}

dependencies {
    // Kotlin stdlib
    implementation("org.jetbrains.kotlin:kotlin-stdlib:1.9.20")

    // Testing
    testImplementation("org.junit.jupiter:junit-jupiter-api:5.9.2")
    testImplementation("org.junit.jupiter:junit-jupiter-engine:5.9.2")
    testImplementation("kotlin.test:kotlin-test-junit5:1.9.20")
    testImplementation("org.jetbrains.kotlin:kotlin-test-junit5:1.9.20")
}

java {
    sourceCompatibility = JavaVersion.VERSION_11
    targetCompatibility = JavaVersion.VERSION_11
}

tasks.test {
    useJUnitPlatform()
    testLogging {
        events("passed", "skipped", "failed")
        exceptionFormat = org.gradle.api.tasks.testing.logging.TestExceptionFormat.FULL
        showStandardStreams = false
    }
}

tasks.withType<org.jetbrains.kotlin.gradle.tasks.KotlinCompile> {
    kotlinOptions.jvmTarget = "11"
    kotlinOptions.freeCompilerArgs = listOf("-Xjsr305=strict")
}

publishing {
    publications {
        create<MavenPublication>("maven") {
            from(components["java"])

            pom {
                name.set("FraiseQL Kotlin")
                description.set("Kotlin authoring language for FraiseQL with 100% feature parity")
                url.set("https://github.com/fraiseql/fraiseql")

                licenses {
                    license {
                        name.set("Apache License 2.0")
                        url.set("https://www.apache.org/licenses/LICENSE-2.0.txt")
                    }
                }

                developers {
                    developer {
                        name.set("FraiseQL Contributors")
                    }
                }
            }
        }
    }
}
