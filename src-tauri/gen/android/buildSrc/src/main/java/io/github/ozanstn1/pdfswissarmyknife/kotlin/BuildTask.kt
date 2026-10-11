import java.io.File
import org.gradle.api.DefaultTask
import org.gradle.api.GradleException
import org.gradle.api.tasks.Input
import org.gradle.api.tasks.TaskAction

/**
 * Verifies that the Rust library for this ABI is present in `jniLibs`.
 *
 * The library itself is compiled by `scripts/build-android.ps1` (or by the
 * Tauri CLI on Linux/macOS via `tauri android build`) before Gradle runs.
 * Building it here would require the Tauri CLI, which on Windows creates a
 * symbolic link and therefore needs Developer Mode, so this task only checks
 * the expected artifact.
 */
open class BuildTask : DefaultTask() {
    @Input
    var rootDirRel: String? = null

    @Input
    var target: String? = null

    @Input
    var release: Boolean? = null

    private val abis = mapOf(
        "aarch64" to "arm64-v8a",
        "armv7" to "armeabi-v7a",
        "i686" to "x86",
        "x86_64" to "x86_64",
    )

    @TaskAction
    fun assemble() {
        val target = target ?: throw GradleException("target cannot be null")
        val abi = abis[target] ?: throw GradleException("unsupported target $target")
        val library = File(project.projectDir, "src/main/jniLibs/$abi/libpdf_sak_lib.so")
        if (!library.isFile) {
            throw GradleException(
                "Missing $library.\n" +
                    "Build the Android library first:\n" +
                    "  npm run android:build            (Windows)\n" +
                    "  npm run tauri android build      (Linux/macOS)"
            )
        }
        logger.info("Using prebuilt Rust library ${library.absolutePath}")
    }
}
