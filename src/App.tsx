import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

interface Clip {
  id: string;
  video_id: string;
  title: string;
  channel: string;
  thumbnail_url: string;
  source_url: string;
  start_time: string;
  end_time: string;
  file_path: string;
  file_size: number;
  created_at: string;
  transcript: string | null;
}

interface ClipResult {
  success: boolean;
  message: string;
  file_path?: string;
  file_size?: string;
  duration_secs: number;
  clip?: Clip;
}

type View = "clip" | "library";

function App() {
  const [view, setView] = useState<View>("clip");
  const [url, setUrl] = useState("");
  const [startTime, setStartTime] = useState("");
  const [endTime, setEndTime] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [result, setResult] = useState<ClipResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [clips, setClips] = useState<Clip[]>([]);
  const [selectedClip, setSelectedClip] = useState<Clip | null>(null);

  useEffect(() => {
    if (view === "library") {
      loadClips();
    }
  }, [view]);

  async function loadClips() {
    try {
      const allClips = await invoke<Clip[]>("get_all_clips");
      setClips(allClips);
    } catch (err) {
      console.error("Failed to load clips:", err);
    }
  }

  async function handleClip(e: React.FormEvent) {
    e.preventDefault();
    setIsLoading(true);
    setResult(null);
    setError(null);

    try {
      const clipResult = await invoke<ClipResult>("download_clip", {
        url,
        start: startTime,
        end: endTime,
      });
      setResult(clipResult);
      // Clear form on success
      if (clipResult.success) {
        setUrl("");
        setStartTime("");
        setEndTime("");
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setIsLoading(false);
    }
  }

  async function openInFinder(path: string) {
    await invoke("open_file_location", { path });
  }

  async function playClip(path: string) {
    await invoke("play_clip", { path });
  }

  async function deleteClip(id: string) {
    if (confirm("Delete this clip?")) {
      try {
        await invoke("delete_clip", { id });
        await loadClips();
        if (selectedClip?.id === id) {
          setSelectedClip(null);
        }
      } catch (err) {
        console.error("Failed to delete clip:", err);
      }
    }
  }

  function formatFileSize(bytes: number): string {
    if (bytes > 1024 * 1024) {
      return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    }
    return `${(bytes / 1024).toFixed(1)} KB`;
  }

  function formatDate(isoString: string): string {
    const date = new Date(isoString);
    return date.toLocaleDateString("en-US", {
      month: "short",
      day: "numeric",
      year: "numeric",
    });
  }

  return (
    <main className="container">
      <header className="header">
        <h1>Clippa</h1>
        <nav className="nav">
          <button
            className={`nav-btn ${view === "clip" ? "active" : ""}`}
            onClick={() => setView("clip")}
          >
            New Clip
          </button>
          <button
            className={`nav-btn ${view === "library" ? "active" : ""}`}
            onClick={() => setView("library")}
          >
            Library
          </button>
        </nav>
      </header>

      {view === "clip" && (
        <div className="clip-view">
          <form onSubmit={handleClip}>
            <div className="form-group">
              <label htmlFor="url">Video URL</label>
              <input
                id="url"
                type="text"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                placeholder="https://youtube.com/watch?v=..."
                required
              />
            </div>

            <div className="time-inputs">
              <div className="form-group">
                <label htmlFor="start">Start Time</label>
                <input
                  id="start"
                  type="text"
                  value={startTime}
                  onChange={(e) => setStartTime(e.target.value)}
                  placeholder="0:00"
                  required
                />
              </div>

              <div className="form-group">
                <label htmlFor="end">End Time</label>
                <input
                  id="end"
                  type="text"
                  value={endTime}
                  onChange={(e) => setEndTime(e.target.value)}
                  placeholder="0:30"
                  required
                />
              </div>
            </div>

            <button type="submit" disabled={isLoading} className="clip-button">
              {isLoading ? "Downloading..." : "Clip"}
            </button>
          </form>

          {error && <div className="error">{error}</div>}

          {result && result.clip && (
            <div className="result">
              <div className="result-header">Clip Saved</div>
              <div className="result-clip">
                <img
                  src={result.clip.thumbnail_url}
                  alt={result.clip.title}
                  className="result-thumbnail"
                />
                <div className="result-info">
                  <div className="result-title">{result.clip.title}</div>
                  <div className="result-channel">{result.clip.channel}</div>
                  <div className="result-stats">
                    <span>{result.file_size}</span>
                    <span>{result.duration_secs}s</span>
                  </div>
                </div>
              </div>
              <div className="result-actions">
                <button onClick={() => playClip(result.clip!.file_path)} className="action-btn">
                  Play
                </button>
                <button onClick={() => openInFinder(result.clip!.file_path)} className="action-btn secondary">
                  Show in Finder
                </button>
              </div>
            </div>
          )}
        </div>
      )}

      {view === "library" && (
        <div className="library-view">
          {clips.length === 0 ? (
            <div className="empty-state">
              <p>No clips yet</p>
              <button onClick={() => setView("clip")} className="clip-button">
                Create your first clip
              </button>
            </div>
          ) : (
            <div className="clips-grid">
              {clips.map((clip) => (
                <div
                  key={clip.id}
                  className={`clip-card ${selectedClip?.id === clip.id ? "selected" : ""}`}
                  onClick={() => setSelectedClip(clip)}
                >
                  <img
                    src={clip.thumbnail_url}
                    alt={clip.title}
                    className="clip-thumbnail"
                  />
                  <div className="clip-info">
                    <div className="clip-title">{clip.title}</div>
                    <div className="clip-channel">{clip.channel}</div>
                    <div className="clip-meta">
                      <span>{clip.start_time} - {clip.end_time}</span>
                      <span>{formatFileSize(clip.file_size)}</span>
                    </div>
                  </div>
                  <div className="clip-actions">
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        playClip(clip.file_path);
                      }}
                      className="icon-btn"
                      title="Play"
                    >
                      <span>&#9658;</span>
                    </button>
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        openInFinder(clip.file_path);
                      }}
                      className="icon-btn"
                      title="Show in Finder"
                    >
                      <span>&#128193;</span>
                    </button>
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        deleteClip(clip.id);
                      }}
                      className="icon-btn danger"
                      title="Delete"
                    >
                      <span>&#128465;</span>
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}

          {selectedClip && (
            <div className="clip-detail">
              <h3>{selectedClip.title}</h3>
              <p className="detail-channel">{selectedClip.channel}</p>
              <div className="detail-meta">
                <div>Clip: {selectedClip.start_time} - {selectedClip.end_time}</div>
                <div>Size: {formatFileSize(selectedClip.file_size)}</div>
                <div>Created: {formatDate(selectedClip.created_at)}</div>
              </div>
              {selectedClip.transcript && (
                <div className="detail-transcript">
                  <h4>Transcript</h4>
                  <p>{selectedClip.transcript}</p>
                </div>
              )}
              <div className="detail-actions">
                <button onClick={() => playClip(selectedClip.file_path)} className="action-btn">
                  Play
                </button>
                <button onClick={() => openInFinder(selectedClip.file_path)} className="action-btn secondary">
                  Show in Finder
                </button>
                <button onClick={() => deleteClip(selectedClip.id)} className="action-btn danger">
                  Delete
                </button>
              </div>
            </div>
          )}
        </div>
      )}
    </main>
  );
}

export default App;
