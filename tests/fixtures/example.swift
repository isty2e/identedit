import Foundation

protocol Processor {
    func processData(_ value: Int) -> Int
}

final class ExampleService: Processor {
    let offset: Int = 1

    func processData(_ value: Int) -> Int {
        return value + offset
    }
}

func helper(_ value: Int) -> Int {
    return value * 2
}
