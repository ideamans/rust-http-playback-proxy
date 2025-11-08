module github.com/pagespeed-quest/http-playback-proxy/accept/golang

go 1.21

require github.com/pagespeed-quest/http-playback-proxy/golang v0.0.0

// Use local version for testing
replace github.com/pagespeed-quest/http-playback-proxy/golang => ../../golang
