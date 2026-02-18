package fixtures.kotlin

import kotlin.math.abs

class ExampleService(private val offset: Int = 1) {
    val label: String = "service"

    fun processData(value: Int): Int {
        return abs(value) + offset
    }
}

fun helper(value: Int): Int {
    return value * 2
}
