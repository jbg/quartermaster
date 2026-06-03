#!/usr/bin/env ruby
# frozen_string_literal: true

require "json"
require "net/http"
require "set"
require "uri"

ALLOWED_LICENSES = Set[
  "Apache-2.0",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "ISC",
  "MIT",
  "MPL-2.0",
  "Ruby",
  "Unicode-3.0",
  "Zlib",
].freeze

lock_path = ARGV.fetch(0, "ios/Gemfile.lock")
specs = []
in_specs = false

File.readlines(lock_path, chomp: true).each do |line|
  if line == "  specs:"
    in_specs = true
    next
  end
  next unless in_specs
  break if line.start_with?("PLATFORMS")

  match = line.match(/^    ([A-Za-z0-9_.-]+) \(([^)]+)\)$/)
  specs << [match[1], match[2]] if match
end

def rubygems_version(name, version)
  uri = URI("https://rubygems.org/api/v2/rubygems/#{URI.encode_www_form_component(name)}/versions/#{URI.encode_www_form_component(version)}.json")
  response = Net::HTTP.get_response(uri)
  raise "RubyGems returned #{response.code}" unless response.is_a?(Net::HTTPSuccess)

  JSON.parse(response.body)
end

failures = []

specs.each do |name, version|
  begin
    metadata = rubygems_version(name, version)
    if metadata["yanked"]
      failures << "#{name} #{version}: gem version is yanked"
    end
    licenses = Array(metadata["licenses"]).compact.map do |license|
      case license.downcase
      when "bsd 2-clause" then "BSD-2-Clause"
      when "bsd 3-clause" then "BSD-3-Clause"
      when "ruby" then "Ruby"
      else license
      end
    end
    if !licenses.empty? && (licenses & ALLOWED_LICENSES.to_a).empty?
      failures << "#{name} #{version}: license is not allowed: #{licenses.join(' OR ')}"
    end
    deprecated = metadata.dig("metadata", "deprecated") || metadata.dig("metadata", "deprecation")
    failures << "#{name} #{version}: deprecated: #{deprecated}" if deprecated && !deprecated.empty?
  rescue StandardError => e
    failures << "#{name} #{version}: #{e.message}"
  end
end

unless failures.empty?
  warn "RubyGems dependency policy violations:"
  failures.sort.each { |failure| warn "- #{failure}" }
  exit 1
end

puts "RubyGems dependency policy ok (#{specs.length} gem versions checked)"
