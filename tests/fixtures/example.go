type Processor struct {
	value int
}

func processData(value int) int {
	return value + 1
}

func (p Processor) helper() int {
	return p.value + 1
}
