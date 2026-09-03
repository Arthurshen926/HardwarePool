plugins {
    `java-library`
}

java {
    sourceCompatibility = JavaVersion.VERSION_17
    targetCompatibility = JavaVersion.VERSION_17
}

tasks.withType<JavaCompile>().configureEach {
    options.release = 17
}

val contractTestOutput = layout.buildDirectory.dir("classes/contractTest")

val compileContractTest by tasks.registering(JavaCompile::class) {
    source(fileTree("src/main/java") { include("**/*.java") })
    source(fileTree("src/contractTest/java") { include("**/*.java") })
    classpath = files()
    destinationDirectory = contractTestOutput
    options.isIncremental = false
    outputs.upToDateWhen { false }
}

tasks.register<JavaExec>("contractTest") {
    group = "verification"
    description = "Runs the no-dependency camera lifecycle contract test."
    dependsOn(compileContractTest)
    classpath = files(contractTestOutput)
    mainClass = "io.capyio.camera.contract.CaptureStateMachineContractTest"
}
