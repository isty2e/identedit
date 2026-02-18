module ExampleApp
  class ExampleService
    VALUE = 1

    def process_data(value)
      value + VALUE
    end
  end
end

def helper(value)
  value * 2
end
