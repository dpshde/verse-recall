import org.gradle.api.publish.maven.MavenPublication

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.serialization")
    id("maven-publish")
}

group = "com.routebible"
version = "0.1.0"

val repositoryRoot = rootDir.resolve("../..").canonicalFile
val rustOutput = layout.buildDirectory.dir("generated/rustJniLibs")

android {
    namespace = "com.routebible.recall"
    compileSdk = 35
    ndkVersion = "27.2.12479018"

    defaultConfig {
        minSdk = 21
        consumerProguardFiles("consumer-rules.pro")

        ndk {
            abiFilters += listOf("arm64-v8a", "armeabi-v7a", "x86_64")
        }

        externalNativeBuild {
            cmake {
                arguments += listOf(
                    "-DRECALL_REPO_ROOT=${repositoryRoot.path}",
                    "-DRECALL_RUST_OUTPUT=${rustOutput.get().asFile.path}",
                    "-DANDROID_STL=c++_static",
                )
                cppFlags += listOf("-std=c++17", "-fexceptions", "-frtti")
            }
        }
    }

    externalNativeBuild {
        cmake {
            path = file("src/main/cpp/CMakeLists.txt")
            version = "3.22.1"
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
        allWarningsAsErrors = true
        freeCompilerArgs += "-Xjvm-default=all"
    }

    testOptions {
        unitTests.isReturnDefaultValues = true
    }

    publishing {
        singleVariant("release") {
            withSourcesJar()
        }
    }
}

dependencies {
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.8.0")

    testImplementation("org.jetbrains.kotlin:kotlin-test-junit:2.1.10")
    testImplementation("junit:junit:4.13.2")
}

val buildRustAndroid by tasks.registering(Exec::class) {
    group = "build"
    description = "Build recall-ffi shared libraries for all configured Android ABIs"
    workingDir(rootDir)
    commandLine(
        "bash",
        rootDir.resolve("scripts/build-rust-android.sh"),
        rustOutput.get().asFile,
    )

    inputs.files(
        repositoryRoot.resolve("Cargo.toml"),
        repositoryRoot.resolve("Cargo.lock"),
        repositoryRoot.resolve("crates/recall-core"),
        repositoryRoot.resolve("crates/recall-ffi"),
        repositoryRoot.resolve("crates/recall-pack"),
    )
    outputs.dir(rustOutput)
}

// CMake links the Rust cdylib, so every native configure/build must follow it.
tasks.configureEach {
    if (name.startsWith("configureCMake") || name.startsWith("externalNativeBuild")) {
        dependsOn(buildRustAndroid)
    }
}

afterEvaluate {
    publishing {
        publications {
            register<MavenPublication>("release") {
                from(components["release"])
                groupId = project.group.toString()
                artifactId = "verse-recall-android"
                version = project.version.toString()
                pom {
                    name.set("Verse Recall for Android")
                    description.set("Offline Bible recall search engine for Android")
                    url.set("https://github.com/route-bible/verse-recall")
                    licenses {
                        license {
                            name.set("MIT License")
                            url.set("https://opensource.org/license/mit")
                        }
                    }
                }
            }
        }
    }
}
