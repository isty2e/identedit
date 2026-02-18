class Processor<T> {
  process(value: T): T {
    return value;
  }
}

function processData(value: number): number {
  return value + 1;
}

const increment = (value: number): number => value + 1;
