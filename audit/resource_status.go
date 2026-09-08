package audit

import "net/url"

// IsBroken reports whether the link result represents a broken link.
func (link Link) IsBroken() bool {
	if link.Result.Kind == ResultFailed {
		return true
	}
	return link.Result.Kind == ResultResponse &&
		link.Result.StatusCode != nil &&
		*link.Result.StatusCode >= 400
}

// IsRedirected reports whether the link resolved to a different request URL.
func (link Link) IsRedirected() bool {
	if (link.Result.Kind != ResultResponse && link.Result.Kind != ResultBlocked) ||
		link.Result.FinalURL == nil {
		return false
	}

	requestURL := link.URL
	if parsed, err := url.Parse(link.URL); err == nil {
		parsed.Fragment = ""
		requestURL = parsed.String()
	}
	return *link.Result.FinalURL != requestURL
}

// IsBroken reports whether the image result represents a broken image.
func (image Image) IsBroken() bool {
	if image.Result.Kind == ResultFailed {
		return true
	}
	return image.Result.Kind == ResultResponse &&
		image.Result.StatusCode != nil &&
		*image.Result.StatusCode >= 400
}

// IsInvalid reports whether the image URL or response content type is invalid.
func (image Image) IsInvalid() bool {
	if image.Result.Kind == ResultInvalid {
		return true
	}
	if image.Result.Kind != ResultResponse ||
		image.Result.StatusCode != nil && *image.Result.StatusCode >= 400 {
		return false
	}
	return !isImageContentType(image.Result.ContentType)
}

// IsRedirected reports whether the image resolved to a different URL.
func (image Image) IsRedirected() bool {
	return (image.Result.Kind == ResultResponse || image.Result.Kind == ResultBlocked) &&
		image.Result.FinalURL != nil &&
		*image.Result.FinalURL != image.URL
}
