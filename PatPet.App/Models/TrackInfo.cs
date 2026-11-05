using Windows.Storage.Streams;

namespace PatPet.Models;

public sealed record TrackInfo(
    string Title,
    string Artist,
    string Album,
    IRandomAccessStreamReference? Thumbnail,
    bool IsPlaying,
    TimeSpan Position,
    TimeSpan Duration,
    DateTimeOffset LastUpdated);
