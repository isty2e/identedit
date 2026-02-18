terraform {
  required_version = ">= 1.6.0"
}

variable "region" {
  type    = string
  default = "us-west-2"
}

locals {
  project = "identedit"
}

resource "aws_s3_bucket" "logs" {
  bucket        = "identedit-logs"
  force_destroy = true
}
