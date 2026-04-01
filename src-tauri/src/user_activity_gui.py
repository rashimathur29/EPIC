# Built-in imports
import os
import sys
from PyQt5.QtWidgets import *
from PyQt5.QtCore import *
from PyQt5.QtWebEngineWidgets import *
from PyQt5.QtWebChannel import QWebChannel
from PyQt5.QtGui import *
from typing import List, Dict
from functools import wraps
from concurrent.futures import ThreadPoolExecutor
import weakref
import logging
import gc
import pytz
from datetime import datetime, timedelta
import uuid
import socket
import platform
import json
import requests
from dateutil.parser import parse
import threading
from PyQt5.QtPositioning import QGeoPositionInfoSource
import psutil
import urllib.parse
import subprocess
import re
import tempfile

# Custom imports
from api_client import ProductivityAPIClient
import integration

# Constants
base_path = getattr(sys, "_MEIPASS", os.path.abspath("."))

# Thread lock for shared resources
threads_lock = threading.Lock()

def ensure_datetime(value):
    if isinstance(value, str):
        try:
            return datetime.fromisoformat(value)
        except ValueError:
            return parse(value)
    return value

def log_function_call(func):
    """Decorator to log entry and exit of a function."""
    logger = logging.getLogger(func.__module__)
    @wraps(func)
    def wrapper(*args, **kwargs):
        logger.debug("Entering %s", func.__name__)
        try:
            result = func(*args, **kwargs)
            logger.debug("Exiting %s", func.__name__)
            return result
        except Exception:
            logger.exception("Exception in function %s", func.__name__)
            raise
    return wrapper


class LoaderLabel(QLabel):
    def __init__(self, parent=None):
        super().__init__(parent)
        self._init_ui()
        self._init_animation()
    
    def _init_ui(self):
        self.setFixedSize(200, 200)
        self.setAlignment(Qt.AlignCenter)
        self.setStyleSheet("background: transparent;")
        if self.parent():
            self._center_in_parent()
    
    def _init_animation(self):
        try:
            loader_path = os.path.join(base_path, "images/setting.gif")
            if os.path.exists(loader_path):
                self.loader_movie = QMovie(loader_path)
                if self.loader_movie.isValid():
                    self.setMovie(self.loader_movie)
                    self.loader_movie.start()
                else:
                    self._create_fallback_animation()
            else:
                self._create_fallback_animation()
        except Exception as e:
            logging.error(f"Error loading animation: {e}")
            self._create_fallback_animation()
        finally:
            gc.collect()
    
    def _center_in_parent(self):
        if not self.parent():
            return
        QTimer.singleShot(0, lambda: self.move(
            (self.parent().width() - self.width()) // 2,
            (self.parent().height() - self.height()) // 2
        ))
    
    def _create_fallback_animation(self):
        self.angle = 0
        self.fallback_pixmap = QPixmap(100, 100)
        self.fallback_pixmap.fill(Qt.transparent)
        painter = QPainter(self.fallback_pixmap)
        painter.setRenderHint(QPainter.Antialiasing)
        painter.setPen(QPen(Qt.blue, 4))
        painter.drawEllipse(10, 10, 80, 80)
        painter.end()
        self.setPixmap(self.fallback_pixmap)
        self.timer = QTimer(self)
        self.timer.timeout.connect(self._update_fallback_animation)
        self.timer.start(50)
    
    def _update_fallback_animation(self):
        self.angle = (self.angle + 10) % 360
        pixmap = QPixmap(100, 100)
        pixmap.fill(Qt.transparent)
        painter = QPainter(pixmap)
        painter.setRenderHint(QPainter.Antialiasing)
        painter.translate(50, 50)
        painter.rotate(self.angle)
        painter.translate(-50, -50)
        painter.setPen(QPen(Qt.blue, 4))
        painter.drawEllipse(10, 10, 80, 80)
        painter.drawLine(50, 20, 50, 40)
        painter.end()
        self.setPixmap(pixmap)
    
    def showEvent(self, event):
        super().showEvent(event)
        self._center_in_parent()
    
    def resizeEvent(self, event):
        super().resizeEvent(event)
        self._center_in_parent()
    
    def cleanup(self):
        if hasattr(self, 'loader_movie'):
            self.loader_movie.stop()
            del self.loader_movie
        if hasattr(self, 'timer'):
            self.timer.stop()
            del self.timer
        gc.collect()

class PrimaryDeviceWorker(QThread):
    finished = pyqtSignal(dict)
    def __init__(self, org_id, email, unique_key, selected_device_id, base_url, user_id, api_cli):
        super().__init__()
        self.org_id = org_id
        self.email = email
        self.unique_key = unique_key
        self.selected_device_id = selected_device_id
        self.base_url = base_url
        self.user_id = user_id
        self.api_cli = api_cli
        with threads_lock:
            self.threads = []
            self.threads.append(self)
    
    def run(self):
        try:
            data = {"email": self.email,
                "unique_key": self.unique_key,
                "selected_device_id": self.selected_device_id}
            #api_client = ProductivityAPIClient(base_url=self.base_url)
            response = self.api_cli.set_primary_device(self.user_id, self.email, self.org_id, data)
            if response.get('success'):
                self.finished.emit({"success": True, "data": json.dumps(response)})
            else:
                self.finished.emit({"success": False, "error": f"Server error: {response}"})
        except Exception as e:
            logging.error(f"Primary device setting error: {str(e)}")
            self.finished.emit({"success": False, "error": str(e)})
        finally:
            gc.collect()
    
    def __del__(self):
        with threads_lock:
            if self in self.threads:
                self.threads.remove(self)
        gc.collect()

class DeviceWorker(QThread):
    finished = pyqtSignal(dict)
    def __init__(self, email, org_id, device_info, base_url, user_id, api_cli):
        super().__init__()
        self.email = email
        self.org_id = org_id
        self.device_info = device_info
        self.base_url = base_url
        self.api_cli = api_cli
        self.user_id = user_id
        with threads_lock:
            self.threads = []
            self.threads.append(self)
    
    def run(self):
        try:
            #api_client = ProductivityAPIClient(base_url=self.base_url)
            response = self.api_cli.register_device(self.user_id, self.email, self.org_id, self.device_info)
            self.finished.emit({"success": True, "data": response})
        except Exception as e:
            logging.error(f"Device registration error: {str(e)}")
            self.finished.emit({"success": False, "error": str(e)})
        finally:
            gc.collect()
    
    def __del__(self):
        with threads_lock:
            if self in self.threads:
                self.threads.remove(self)
        gc.collect()

class Worker(QObject):
    # Use object to allow any return type (None, dict, list, etc.) from task functions
    finished = pyqtSignal(object)
    error = pyqtSignal(str)
    def __init__(self, task_func, *args, **kwargs):
        super().__init__()
        self.task_func = task_func
        self.args = args
        self.kwargs = kwargs
    
    @pyqtSlot()
    def run(self):
        try:
            result = self.task_func(*self.args, **self.kwargs)
            self.finished.emit(result)
        except Exception as e:
            logging.error(f"Worker error: {str(e)}")
            self.error.emit(str(e))
        finally:
            gc.collect()

class AuthWorker(QObject):
    finished = pyqtSignal(dict)
    def __init__(self, email, unique_key, password, org_id, device_name, TRIAL_URL):
        super().__init__()
        self.email = email
        self.unique_key = unique_key
        self.password = password
        self.org_id = org_id
        self.device_name = device_name
        self.TRIAL_URL = TRIAL_URL
    
    def run(self):
        try:
            api_client = os.getenv("LOGIN_URL")
            auth_response = requests.post(api_client, json={"email": self.email, "password": self.password, "productName": "epic"})
            self.finished.emit({"success": True, "data": auth_response.json()})
        except Exception as e:
            logging.error(f"Authentication error: {str(e)}")
            self.finished.emit({"success": False, "error": str(e)})
        finally:
            gc.collect()

class DisclaimerDialog(QDialog):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWindowFlags(self.windowFlags() & ~Qt.WindowContextHelpButtonHint)
        self.setWindowTitle("Disclaimer")
        self.setFixedSize(400, 300)
        self.setModal(True)
        layout = QVBoxLayout()
        disclaimer_text = QLabel(
            "<b>By proceeding, you acknowledge and agree that:</b><br><br>"
            "<ul>"
            "<li>The EPIC Desktop Agent will capture mouse movements, screen activity, screenshots, and short video clips during business hours.</li>"
            "<li>No personal information or passwords will be collected.</li>"
            "<li>All captured data remains within your organization and is used solely for productivity monitoring and reporting.</li>"
            "<li>You consent to this monitoring as part of your use of EPIC.</li>"
            "</ul>")
        disclaimer_text.setWordWrap(True)
        disclaimer_text.setTextFormat(Qt.RichText)  # Ensure HTML rendering
        layout.addWidget(disclaimer_text)
        self.agree_checkbox = QCheckBox("I agree to the terms and monitoring policy")
        layout.addWidget(self.agree_checkbox)
        button_layout = QHBoxLayout()
        ok_button = QPushButton("Proceed")
        ok_button.clicked.connect(self.on_proceed)
        cancel_button = QPushButton("Cancel")
        cancel_button.clicked.connect(self.reject)
        button_layout.addWidget(ok_button)
        button_layout.addWidget(cancel_button)
        layout.addLayout(button_layout)
        self.setLayout(layout)

    def on_proceed(self):
        if self.agree_checkbox.isChecked():
            self.accept()
        else:
            QMessageBox.warning(self,"Agreement Required","You must agree to the terms to proceed.")

class UpdateDialog(QDialog):
    updateRequested = pyqtSignal()
    
    def __init__(self):
        super().__init__()
        self.setWindowTitle("⚠️ Update Required")
        self.setModal(True)
        self.setWindowFlags(Qt.Window | Qt.CustomizeWindowHint | Qt.WindowTitleHint | Qt.WindowStaysOnTopHint)
        self.setFixedSize(400, 200)
        self.setStyleSheet("""
            QDialog {background-color: #ffffff; border: 2px solid #0078d4; border-radius: 12px;}
            QLabel#titleLabel {font-size: 18px; font-weight: bold; color: #333; padding: 10px;}
            QLabel#infoLabel {font-size: 15px; color: #555; padding: 10px;}
            QPushButton {background-color: #0078d4; color: white; font-size: 14px; padding: 10px 30px; margin: 10px; border-radius: 6px;}
            QPushButton:hover {background-color: #005a9e;}
            QPushButton:pressed {background-color: #003d6b;}
        """)
        layout = QVBoxLayout()
        title = QLabel("⚠️ Update Required")
        title.setObjectName("titleLabel")
        title.setAlignment(Qt.AlignCenter)
        layout.addWidget(title)
        message = QLabel("A new version of the Productivity Monitor Application is available. Please update to continue.")
        message.setObjectName("infoLabel")
        message.setAlignment(Qt.AlignCenter)
        message.setWordWrap(True)
        layout.addWidget(message)
        self.update_button = QPushButton("Update Now")
        self.update_button.clicked.connect(self.on_update_requested)
        layout.addWidget(self.update_button, alignment=Qt.AlignCenter)
        self.setLayout(layout)
    
    def on_update_requested(self):
        self.accept()
        self.updateRequested.emit()
    
    def closeEvent(self, event):
        event.ignore()
        gc.collect()

class RefreshThread(QThread):
    finished_signal = pyqtSignal(bool)
    def __init__(self, db_manager):
        super().__init__()
        self.db_manager = db_manager
        with threads_lock:
            self.threads = []
            self.threads.append(self)

    def run(self):
        try:
            result= self.db_manager.get_data()
            self.finished_signal.emit(result)
        except Exception as e:
            logging.error(f"Error in RefreshThread: {e}")
            self.finished_signal.emit(False)
        finally:
            gc.collect()

    def __del__(self):
        with threads_lock:
            if self in self.threads:
                self.threads.remove(self)
        gc.collect()


class LogActivity(QObject):
    tourChanged = pyqtSignal()
    BUTTON_STATES = {
        'enabled': {
            'checkin': {'background': 'linear-gradient(128deg, rgba(76, 175, 80, 1) 0%, rgba(44, 215, 42, 1) 100%)', 'border': 'solid 1px #3bc43c', 'cursor': 'pointer', 'text_color': '#ffffff'},
            'checkout': {'background': 'linear-gradient(128deg, rgba(230, 57, 70, 1) 0%, rgba(235, 54, 56, 1) 100%)', 'border': 'solid 1px #e8373f', 'cursor': 'pointer', 'text_color': '#ffffff'},
            'pause': {'background': 'linear-gradient(128deg, rgba(255, 140, 0, 0.769) 0%, rgba(255, 195, 0, 1) 100%)', 'border': 'solid 1px #bac724', 'cursor': 'pointer', 'text_color': '#ffffff'},
            'meeting': {'background': 'linear-gradient(99.16deg, #4CAF50 2.04%, #2CD72A 97.96%)', 'border': 'solid 1px #3BC43C', 'cursor': 'pointer', 'text_color': '#ffffff'},
            'default': {'background': '#407BFF', 'border': 'solid 1px #407BFF', 'cursor': 'pointer', 'text_color': '#ffffff'}
        },
        'disabled': {'background': "#bca7a7bc", 'border': 'solid 1px #d0d0d0', 'cursor': 'not-allowed', 'color': "#50484805", 'opacity': '1.0'}
    }
    def __init__(self, tracker, browser, parent, log_file,mail_file, version_file, db_manager, api_cli):
        super().__init__()
        self.tracker = tracker
        self.browser = browser
        self.parent = parent
        self.pause_start_time = None
        
        self.log_file_path = log_file
        self.mail_file = mail_file
        self.version_file = version_file
        self.db_manager = db_manager
        self.is_checked_in = False
        self.is_paused = False
        self.pause_completed = False
        self.responses = []
        self.is_checked_in = False
        self.meeting_start = None
        self._show_tour = False
        self.temp_file_path = None
        self.validity_timer = None
        with threads_lock:
            self.threads = []

        with open(self.mail_file, 'r') as f:
            self.email = f.read().strip()
        print(f"User email loaded: {self.email}")
        print(f"Base URL from config: {self.db_manager.get_single_config('base_url')}")
        print(f"Org ID from DB: {self.db_manager.get_org_id()}")
        self.api_cli = ProductivityAPIClient(self.db_manager.get_single_config("base_url"), self.email, self.db_manager.get_userid(), self.db_manager.get_org_id().strip())

    def handle_resume(self):
        self.reason = "System Shutdown"
        self.tracker.is_paused = False
        self.tracker.pause_start_time = self.db_manager.get_pause_start_time()
        self.tracker.pause_end_time = datetime.now(pytz.utc).replace(tzinfo=None, microsecond=0)
        checkin_time, was_paused = self.db_manager.get_pause_state()
        # Notify JS to (re)start/resync its elapsed timer rather than updating from Python
        '''try:
            self.update_ui_elapsed_time()
        except Exception:
            logging.exception('Failed to invoke JS elapsed timer')'''
        # If the DB indicates there was an active pause, persist the break end
        # and then use the unified resume path. resume_activity will handle
        # tracker/monitor resume and UI updates. We pass skip_db=True when the
        # DB write has already been performed here to avoid double-logging.
        try:
            if was_paused:
                self.db_manager.log_break_end(checkin_time, datetime.now(pytz.utc).replace(tzinfo=None, microsecond=0))
                # DB write done, let resume_activity know to skip another DB write
                self.resume_activity(skip_db=True)
            else:
                # Use unified path so UI updates and tracker.resume are performed
                self.resume_activity()
        except Exception:
            logging.exception('Error during handle_resume resume flow')
        finally:
            self.is_paused = False

    def enableUpdateButton(self):
        button_states = {
            "update-button": {
                "disabled": False,
                "cursor": "pointer"
            }
        }
        self.update_button_states(button_states)

    @pyqtSlot()
    def update_app(self):
        logging.info("Starting app update process")
        QTimer.singleShot(0, self._start_update_process)

    def _start_update_process(self):
        self.browser.page().runJavaScript("document.getElementById('update-button').style.display = 'none';")
        self.browser.page().runJavaScript("document.getElementById('download-gif').style.display = 'block';")
        try:
            version_url = os.getenv('VERSION_URL')
            os_name = platform.system().lower()
            params = {'productName': os.getenv('PRODUCT_NAME'), 'platform': os_name}
            response = requests.get(url=version_url, params=params, timeout=15)
            download_url = response.json().get("url")
            if hasattr(self, "_update_thread") and self._update_thread and self._update_thread.is_alive():
                logging.info("Update thread already running, not starting another.")
                return
            self._update_thread = threading.Thread(target=self.download_and_install, args=(download_url,), daemon=True)
            with threads_lock:
                self.threads.append(self._update_thread)
            self._update_thread.start()
        except Exception as e:
            logging.error(f"Error starting update process: {e}")
        finally:
            gc.collect()

    def download_and_install(self, download_url):
        try:
            temp_dir = tempfile.gettempdir()
            timestamp = datetime.now().strftime("%Y%m%d%H%M%S")
            installer_path = os.path.join(temp_dir, f"EPIC.exe")
            self.temp_file_path = installer_path
            with requests.get(download_url, stream=True) as response:
                response.raise_for_status()
                with open(installer_path, 'wb') as f:
                    for chunk in response.iter_content(chunk_size=8192):
                        if chunk:
                            f.write(chunk)
                logging.info(f"Installer downloaded to: {installer_path}")
                try:
                    if platform.system() == "Windows":
                        subprocess.run(["taskkill", "/f", "/im", "watchdog.exe"], 
                                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                        logging.info("watchdog.exe process terminated")
                    else:
                        # For non-Windows systems (though unlikely for watchdog.exe)
                        subprocess.run(["pkill", "-f", "watchdog"], 
                                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                except Exception as e:
                    logging.warning(f"Failed to kill watchdog.exe: {e}")
                subprocess.Popen([installer_path, "/silent", "/norestart"], shell=False)
                logging.info("Exiting current application for update...")
                os._exit(0)
        except Exception as e:
            logging.error(f"Unexpected error during download/install: {e}")
        finally:
            gc.collect()
        
    @pyqtSlot()
    def update_button_states(self, state_dict):
        js_parts = []
        for button_id, state in state_dict.items():
            disabled = state.get('disabled', False)
            js_parts.append(f"""
                var button = document.getElementById('{button_id}');
                if (button) {{
                    button.disabled = {str(disabled).lower()};
                    button.style.background = '{state.get('background', '')}';
                    button.style.border = '{state.get('border', '')}';
                    button.style.cursor = '{state.get('cursor', '')}';
                    button.style.pointerEvents = '{'none' if disabled else 'auto'}';
                    button.tabIndex = {'-1' if disabled else '0'};
                    localStorage.setItem('{button_id}State', '{'disabled' if disabled else 'enabled'}');
                }}
            """)
        self.browser.page().runJavaScript("".join(js_parts))
        gc.collect()

    @pyqtSlot(str, int)
    def request_long_break_approval(self, reason, duration):
        """
        Called from JS when a break longer than 45 minutes requires approval.
        """
        try:
            logging.info(f"Long break approval requested | Reason: {reason} | Duration: {duration} minutes")

            # Forward approval request to API
            response = self.api_cli.send_for_approval(reason, duration, True)

            status = response.get('status', 'unknown')
            message = response.get('message', '')

            if status == 'approved':
                logging.info(f"Break approval granted | Reason: {reason} | Duration: {duration} minutes")
                QMessageBox.information(
                    None,
                    "Break Approved",
                    f"Your break request has been approved.\n\n"
                    f"Requested Duration: {duration} minutes\n"
                    f"Reason: {reason}"
                )
            elif status == 'declined':
                logging.warning(f"Break approval declined | Reason: {reason} | Duration: {duration} minutes | Message: {message}")
                QMessageBox.warning(
                    None,
                    "Break Declined",
                    f"Your break request has been declined.\n\n"
                    f"Requested Duration: {duration} minutes\n"
                    f"Reason: {reason}"
                )
            else:
                logging.info(f"Break approval status unknown | Status: {status} | Reason: {reason} | Duration: {duration} minutes")
                QMessageBox.information(
                    None,
                    "Approval Status",
                    f"Break approval status: {status}.\n\n"
                    f"Requested Duration: {duration} minutes\n"
                    f"Reason: {reason}"
                )

        except Exception as e:
            logging.error(f"Error handling long break approval: {e}")
            QMessageBox.critical(
                None,
                "Approval Error",
                f"An error occurred while processing your break approval request.\n\n"
                f"Requested Duration: {duration} minutes\n"
                f"Reason: {reason}\n\n"
                f"Error: {str(e)}"
            )


    @pyqtSlot(str)
    def handle_pause_with_reason(self, reason: str):
        """Pause slot that accepts a reason string from JS (e.g., Add Timer reason).

        JS should call `backend.handle_pause_with_reason(reason)` when it has a
        user-provided reason — this ensures the reason is persisted to DB.
        """
        try:
            if reason is None:
                reason = ""
            self._perform_immediate_pause(reason)
        except Exception:
            logging.exception('Error in handle_pause_with_reason')

    def _perform_immediate_pause(self, reason=None):
        """Internal helper to perform an immediate (dialog-less) pause.

        If `reason` is provided it will be used; otherwise a default
        reason is used for backwards compatibility.
        """
        try:
            if self.pause_completed:
                logging.info("_perform_immediate_pause called but pause already completed; ignoring request")
                return

            if self.is_paused:
                logging.info("Already paused; calling resume_activity() to end pause")
                try:
                    self.resume_activity()
                except Exception:
                    logging.exception('Error while resuming from _perform_immediate_pause')
                return

            # Use provided reason when available
            self.reason = reason.strip() if reason else "Manual Pause"
            self.pause_start_time = datetime.now(pytz.utc).replace(tzinfo=None, microsecond=0)
            self.is_paused = True
            self.pause_completed = False
            self.db_manager.log_break_with_no_end(self.db_manager.get_checkin_time(), self.pause_start_time, self.reason)

            # Let tracker record the pause (tracker handles DB logging)
            try:
                if hasattr(self, 'tracker') and self.tracker:
                    self.tracker.pause(self.pause_start_time, self.reason)
            except Exception:
                logging.exception('Error invoking tracker.pause from _perform_immediate_pause')

            # Update the embedded page UI to reflect paused state
            try:
                js = f"""
                try {{
                    var pauseBtn = document.getElementById('pause-button');
                    if (pauseBtn) {{
                        var textNode = pauseBtn.lastChild;
                        if (textNode && textNode.nodeType === Node.TEXT_NODE) textNode.nodeValue = 'Resume';
                        pauseBtn.style.background = 'linear-gradient(128deg, rgba(255, 140, 0, 0.769) 0%, rgba(255, 195, 0, 1) 100%)';
                        pauseBtn.style.border = 'solid 1px #bac724';
                    }}
                    localStorage.setItem('isPaused', 'true');
                    localStorage.setItem('pauseReason', '{self.reason}');
                    localStorage.setItem('pauseStartTime', (new Date()).getTime());

                    var checkOut = document.getElementById('checkOutBtn');
                    if (checkOut) {{ checkOut.disabled = true; checkOut.style.opacity = '0.5'; checkOut.style.cursor = 'not-allowed'; }}
                    var meet = document.getElementById('meeting-button');
                    if (meet) {{ meet.disabled = true; meet.style.opacity = '0.5'; meet.style.cursor = 'not-allowed'; }}
                }} catch (err) {{ console.error('handle_pause JS update error', err); }}
                """
                if hasattr(self, 'browser') and self.browser:
                    self.browser.page().runJavaScript(js)
            except Exception:
                logging.exception('Failed to update page UI from _perform_immediate_pause')

            logging.info('Immediate pause completed')
        except Exception as e:
            logging.exception(f'Unhandled error in _perform_immediate_pause: {e}')
        finally:
            gc.collect()

    @pyqtSlot()
    def handle_pause(self):
        """Legacy no-arg pause slot kept for compatibility; uses default reason."""
        self._perform_immediate_pause(None)
    
    @pyqtSlot(int)
    def handle_pause_end_ms(self, ms: int):
        """Optional timestamped slot for JS to call with a millisecond epoch.
        This helps with deterministic matching from the client. It delegates to
        the existing no-arg handler after storing the incoming value for
        diagnostic purposes.
        """
        try:
            logging.info(f'handle_pause_end_ms called from JS with ms={ms}')
            try:
                self._incoming_pause_end_ms = int(ms)
            except Exception:
                logging.exception('Failed to store incoming pause_end ms')
            # Delegate to the primary handler which performs DB-driven fallback
            self.handle_pause_end()
        except Exception:
            logging.exception('Error in handle_pause_end_ms')
        finally:
            gc.collect()

    @pyqtSlot()
    def handle_pause_end(self):
        """Called from JS when a break timer completes — ends the pause and
        ensures it cannot be resumed again."""
        try:
            logging.info('handle_pause_end called from JS — ending pause')
            if not self.is_paused:
                logging.info('No active pause flag set on backend; attempting DB-driven close of any active break')
                try:
                    checkin_time = self.db_manager.get_checkin_time()
                    pause_start = self.db_manager.get_pause_start_time()
                    '''if not pause_start:
                        logging.info('No active break found in DB to close.')
                        return'''
                    # Parse pause_start (DB returns string), ensure timezone handling
                    try:
                        pause_start_dt = ensure_datetime(pause_start)
                    except Exception:
                        logging.exception('Failed to parse pause_start from DB')
                        pause_start_dt = None

                    pause_end_time = datetime.now(pytz.utc).replace(tzinfo=None, microsecond=0)
                    # Ensure backend/trackers are marked paused so resume() will run
                    if pause_start_dt:
                        try:
                            # Mark GUI/backend state so tracker.resume won't early-exit
                            self.is_paused = True
                            self.pause_start_time = pause_start_dt
                        except Exception:
                            logging.exception('Failed to set GUI pause flags')
                        try:
                            if hasattr(self, 'tracker') and self.tracker:
                                self.tracker.is_paused = True
                                self.tracker.pause_start_time = pause_start_dt
                        except Exception:
                            logging.exception('Failed to set tracker pause flags')
                        try:
                            if hasattr(self.tracker, 'monitor') and self.tracker.monitor:
                                self.tracker.monitor.is_paused = True
                                self.tracker.monitor.pause_start_time = pause_start_dt
                        except Exception:
                            logging.exception('Failed to set monitor pause flags')

                    # Attempt to persist break end in DB
                    try:
                        success = self.db_manager.log_break_end(checkin_time, pause_end_time)
                        logging.info(f'Fallback DB log_break_end result: {success}')
                    except Exception:
                        logging.exception('Fallback log_break_end failed')
                        success = False

                    # Save the fallback DB result so resume_activity can reuse it
                    try:
                        self._last_fallback_db_success = success
                    except Exception:
                        logging.exception('Failed to set _last_fallback_db_success')

                    # Use the unified resume path so UI updates and tracker resume
                    try:
                        logging.info('Calling resume_activity(skip_db=True) from DB-driven fallback')
                        self.resume_activity(skip_db=True)
                    except Exception:
                        logging.exception('Error while calling resume_activity in fallback path')

                    # Mark completed so this pause cannot be resumed again
                    self.pause_completed = True
                    return
                except Exception:
                    logging.exception('Unhandled error during DB-driven pause end fallback')
                    return

            # End the pause (this will log break end and call tracker.resume)
            try:
                self.resume_activity()
            except Exception:
                logging.exception('Error while ending pause from handle_pause_end')

            # Mark the pause as completed so it can't be resumed again
            self.pause_completed = True
        except Exception as e:
            logging.exception(f'Unhandled error in handle_pause_end: {e}')
        finally:
            gc.collect()

    def resume_activity(self, skip_db: bool = False):
        """Resume monitoring/UI. If skip_db is True, the DB write has already
        been performed by the caller (fallback path) and the method should only
        perform tracker/monitor resume and UI updates.
        """
        try:
            logging.info("Resuming activity")
            self.is_paused = False
            pause_end_time = datetime.now(pytz.utc).replace(tzinfo=None, microsecond=0)
            success = False
            if not skip_db:
                checkin = self.db_manager.get_checkin_time()
                success = self.db_manager.log_break_end(checkin, pause_end_time)
            else:
                success = getattr(self, '_last_fallback_db_success', False)
            try:
                if hasattr(self, 'tracker') and self.tracker:
                    self.tracker.resume(self.pause_start_time, pause_end_time, success)
                elif hasattr(self, 'tracker') and getattr(self.tracker, 'monitor', None):
                    # fallback: call monitor.resume directly if tracker missing
                    try:
                        self.tracker.monitor.resume(pause_end_time)
                    except Exception:
                        logging.exception('Failed to call monitor.resume from resume_activity')
            except Exception:
                logging.exception('Error while calling tracker/monitor resume in resume_activity')
            if hasattr(self, 'resume_timer'):
                self.resume_timer.stop()
            self.pause_start_time = None
            self.reason = None
            # Clear any transient fallback marker
            if hasattr(self, '_last_fallback_db_success'):
                try:
                    delattr(self, '_last_fallback_db_success')
                except Exception:
                    try:
                        del self._last_fallback_db_success
                    except Exception:
                        pass
            # Ensure the UI reflects the resumed state: re-enable checkout/meeting
            # buttons and restore the pause button label. Use the reusable helper
            # where possible and also run a small JS snippet to ensure the
            # in-page text and localStorage are consistent.
            try:
                # Re-enable the primary action buttons
                self.update_button_states({
                    'checkOutBtn': {**self.BUTTON_STATES['enabled']['checkout'], 'disabled': False},
                    'pause-button': {**self.BUTTON_STATES['enabled']['pause'], 'disabled': False},
                    'meeting-button': {**self.BUTTON_STATES['enabled']['meeting'], 'disabled': False}
                })
                # Restore pause button label and clear paused flag in localStorage
                js_restore = """
                try {
                    var pauseBtn = document.getElementById('pause-button');
                    if (pauseBtn) {
                        var textNode = pauseBtn.lastChild;
                        if (textNode && textNode.nodeType === Node.TEXT_NODE) textNode.nodeValue = 'Pause';
                        pauseBtn.style.pointerEvents = 'auto';
                        pauseBtn.tabIndex = 0;
                    }
                    localStorage.setItem('isPaused', 'false');
                    var out = document.getElementById('checkOutBtn'); if (out) { out.disabled = false; out.style.opacity = ''; out.style.cursor = 'pointer'; }
                    var meet = document.getElementById('meeting-button'); if (meet) { meet.disabled = false; meet.style.opacity = ''; meet.style.cursor = 'pointer'; }
                } catch (e) { console.warn('resume UI update failed', e); }
                """
                if hasattr(self, 'browser') and self.browser:
                    try:
                        self.browser.page().runJavaScript(js_restore)
                    except Exception:
                        logging.exception('Failed to run resume UI JS')
                # Ensure the embedded page knows the session is active without
                # altering the existing displayed stopwatch value. We set the
                # client-side running flag and call the stopwatch tick initializer
                # if available so features that gate on appState.stopwatchRunning
                # (like adding break timers) are allowed after resume.
                try:
                    js_resume = """
                    try {
                        if (typeof appState !== 'undefined') {
                            appState.stopwatchRunning = true;
                            appState.isPaused = false;
                        }
                        if (typeof startStopwatchTimer === 'function') {
                            try { startStopwatchTimer(); } catch(e) { console.warn('startStopwatchTimer call failed', e); }
                        }
                        if (typeof updateButtonStates === 'function') {
                            try { updateButtonStates(); } catch(e) { console.warn('updateButtonStates failed', e); }
                        }
                        try { localStorage.setItem('isPaused', 'false'); } catch(e) {}
                    } catch (e) { console.warn('resume client update failed', e); }
                    """
                    if hasattr(self, 'browser') and self.browser:
                        try:
                            self.browser.page().runJavaScript(js_resume)
                        except Exception:
                            logging.exception('Failed to run resume client JS')
                except Exception:
                    logging.exception('Failed to inject client resume JS')
            except Exception:
                logging.exception('Failed to update UI after resume')
        except Exception as e:
            logging.error(f"Error resuming activity: {e}")
        finally:
            gc.collect()

    @pyqtSlot(result='QVariant')
    def get_todays_breaks(self):
        """Return the list of breaks for today's most recent checkin as JSON string.

        This is used by the embedded JS page on load to render persisted break
        timers (both active and completed) so the user sees all today's breaks.
        """
        try:
            logging.info('get_todays_breaks called from JS')
            checkin = self.db_manager.get_checkin_time()
            logging.debug(f'get_todays_breaks: raw checkin from DB: {checkin}')
            if not checkin:
                logging.info('get_todays_breaks: no active checkin found')
                return json.dumps([])

            # Ensure checkin is a datetime string in the expected format
            if isinstance(checkin, datetime):
                checkin_key = checkin.strftime("%Y-%m-%d %H:%M:%S")
            else:
                checkin_key = str(checkin)

            logging.debug(f'get_todays_breaks: using checkin key: {checkin_key}')
            breaks = self.db_manager.get_breaks(checkin_key)
            logging.info(f'get_todays_breaks: fetched breaks count={len(breaks) if isinstance(breaks, list) else "?"}')
            # Return native Python list/dict structures so QWebChannel maps them to
            # real JS arrays/objects on the frontend rather than double-encoded JSON strings.
            return breaks
        except Exception as e:
            logging.exception(f"Failed to fetch today's breaks: {e}")
            return json.dumps([])
    @pyqtSlot('QVariant')
    def notify_elapsed_payload(self, payload):
        """Called from JS via QWebChannel to acknowledge receipt of the elapsed payload.

        This helps debug timing issues where Python injects the payload before the
        client's scripts are ready. The `payload` will be a JSON-like object or null.
        """
        try:
            logging.info(f"[JS ACK] startElapsedTimerFromPython payload received by JS: {payload}")
        except Exception:
            logging.exception("Error logging JS ACK payload")
        finally:
            gc.collect()
        
    @pyqtSlot(result=str)
    def get_activity_logs(self):
        """Fetch recent activity logs from DB and return as JSON string for JS."""
        try:
            logs = self.db_manager.get_activity_logs(limit=10)  # Implement this in DBManager if missing
            formatted_logs = []
            for log_entry in logs:
                # Assume log_entry is a tuple/list: (timestamp_str, message)
                timestamp = log_entry[0] if log_entry[0] else datetime.now().strftime("%H:%M:%S")
                message = log_entry[1] if len(log_entry) > 1 else "Unknown activity"
                formatted_logs.append({
                    "time": f"[{timestamp}]",  # Extract time part, e.g., "[10:30:45]"
                    "message": message
                })
            formatted_logs.sort(key=lambda x: x["time"], reverse=True)
            return json.dumps(formatted_logs)
        except Exception as e:
            logging.error(f"Error fetching activity logs: {e}")
            return json.dumps([])  # Return empty list on error
        
    @pyqtSlot(int, str)
    def record_meeting_start(self, start_time, meeting_type):
        """Record a meeting in the database."""
        try:
            start_time = datetime.now(pytz.utc)
            start_time = start_time.replace(tzinfo=None, microsecond=0)
            # Log as inactivity entry with meeting type as reason
            self.db_manager.log_call_start(start_time, meeting_type)
            self.pause_thread = threading.Thread(target=self.tracker.pause_for_meet, name="PauseThread")
            self.pause_thread.start()
        except Exception as e:
            logging.error(f"Error recording meeting: {e}")
    
    @pyqtSlot(str, int)
    def record_meeting(self, meeting_type, duration_ms):
        """Record a meeting in the database."""
        try:
            # Calculate start time from duration
            end_time = datetime.now(pytz.utc).replace(tzinfo=None, microsecond=0)
            start_time = end_time - timedelta(milliseconds=duration_ms)
            self.db_manager.log_call_end(start_time, end_time)
            self.tracker.resume_post_meet()
            logging.info(f"Meeting recorded: type={meeting_type}, duration={duration_ms}ms")
        except Exception as e:
            logging.error(f"Error recording meeting: {e}")

    @pyqtSlot(result='QVariant')
    def get_meetings(self):
        """Retrieve past meetings from database (reason != 'online')."""
        try:
            logging.info('get_meetings called from JS')
            meetings = self.db_manager.get_meetings()
            logging.info(f'get_meetings: fetched {len(meetings) if isinstance(meetings, list) else "?"} meetings')
            # Sanitize meetings so everything is JSON-serializable (convert datetimes)
            sanitized = []
            try:
                for m in meetings or []:
                    start = m.get('start_time')
                    end = m.get('end_time')
                    duration = m.get('duration')
                    mtype = m.get('type')
                    # Convert datetimes to string if necessary so JS Date parsing is consistent
                    if isinstance(start, datetime):
                        start = start.strftime("%Y-%m-%d %H:%M:%S")
                    if isinstance(end, datetime):
                        end = end.strftime("%Y-%m-%d %H:%M:%S")
                    sanitized.append({
                        'start_time': start,
                        'end_time': end,
                        'duration': duration,
                        'type': mtype
                    })
                # Return native Python list/dicts so QWebChannel marshals to JS arrays/objects
                return sanitized
            except Exception:
                logging.exception('Failed to sanitize meetings for JSON; returning raw meetings as fallback')
                try:
                    # Ensure fallbacks are native types where possible
                    return meetings
                except Exception:
                    return []
        except Exception as e:
            logging.exception(f"Error retrieving meetings: {e}")
            return []

    @pyqtSlot(result='QVariant')
    def get_test_meetings(self):
        """Return a deterministic small meetings list (for UI marshalling tests)."""
        try:
            test = [
                {
                    'start_time': datetime.now().strftime("%Y-%m-%d %H:%M:%S"),
                    'end_time': None,
                    'duration': '00:10:00',
                    'type': 'team-test'
                }
            ]
            return test
        except Exception:
            logging.exception('Failed to produce test meetings payload')
            return json.dumps([])
        
    @pyqtSlot()
    def handle_signout_org(self):
        try:
            if self.is_checked_in:
                QMessageBox.warning(self.parent.root,"Sign Out","You are currently checked in. Please check out before signing out.")
                return
            reply = QMessageBox.question(self.parent.root,"Confirm Sign Out","Are you sure you want to sign out of the organization?",QMessageBox.Yes | QMessageBox.No,QMessageBox.No)
            if reply != QMessageBox.Yes:
                return
            self.db_manager.clear_org_data()
        except Exception as e:
            logging.info(f"Error clearing email file: {e}", exc_info=True)

        try:
            QMessageBox.information(self.parent.root, "Sign Out", "You have been signed out of the organization.")
        except Exception:
            pass
        self.parent.root.close
        os._exit(0)

    @pyqtSlot()
    def handle_signout(self):
        try:
            if self.is_checked_in:
                QMessageBox.warning(self.parent.root,"Sign Out","You are currently checked in. Please check out before signing out.")
                return
            reply = QMessageBox.question(self.parent.root,"Confirm Sign Out","Are you sure you want to sign out?",QMessageBox.Yes | QMessageBox.No,QMessageBox.No)
            if reply != QMessageBox.Yes:
                return
            with open(self.mail_file, "w") as f:
                f.write("")
        except Exception as e:
            logging.info(f"Error clearing email file: {e}", exc_info=True)

        try:
            QMessageBox.information(self.parent.root, "Sign Out", "You have been signed out.")
        except Exception:
            pass
        self.parent.root.close
        os._exit(0)
    
    @pyqtSlot()
    def markTourComplete(self):
        """Call this when tour is completed"""
        try:
            self._show_tour = False
            self.tourChanged.emit()
            self.api_cli.mark_first_login_complete(
                self.db_manager.get_user_id(), 
                self.db_manager.get_userid(),
                self.db_manager.get_org_id()
            )
            
            logging.info("Tour marked as completed")
        except Exception as e:
            logging.info(f"Error marking tour complete: {e}")
        
    @pyqtSlot()
    def create_update_screen(self):
        try:
            logging.info("Creating update screen")
            parent_widget = self.parent.root
            dialog = QDialog(parent_widget)
            dialog.setWindowFlags(Qt.Dialog | Qt.CustomizeWindowHint | Qt.WindowTitleHint | Qt.WindowStaysOnTopHint)
            dialog.setModal(True)
            dialog.setFixedSize(500, 400)
            dialog.setWindowTitle("Update")
            dialog.setStyleSheet("background-color: white;")

            layout = QVBoxLayout()

            # Logo
            logo = QLabel()
            logo_path = os.path.join(base_path, "images/logo.png")
            if os.path.exists(logo_path):
                pixmap = QPixmap(logo_path)
                logo.setPixmap(pixmap.scaled(80, 30, Qt.KeepAspectRatio, Qt.SmoothTransformation))
            else:
                logging.warning(f"Logo image not found at {logo_path}")
            layout.addWidget(logo, alignment=Qt.AlignLeft)
            layout.addSpacing(20)

            # Rocket image
            rocket = QLabel()
            rocket_path = os.path.join(base_path, "images/rocket.png")
            if os.path.exists(rocket_path):
                pixmap = QPixmap(rocket_path)
                rocket.setPixmap(pixmap.scaled(200, 200, Qt.KeepAspectRatio, Qt.SmoothTransformation))
            else:
                logging.warning(f"Rocket image not found at {rocket_path}")
            layout.addWidget(rocket, alignment=Qt.AlignCenter)

            # Text
            text = QLabel("We're getting better!\nUpdate the app to unlock new features")
            text.setAlignment(Qt.AlignCenter)
            text.setFont(QFont("Arial", 16))
            text.setStyleSheet("color: #333333;")
            layout.addWidget(text)
            layout.addSpacing(30)

            # Update button
            update_button = QPushButton("Update Now")
            update_button.setStyleSheet("""
                QPushButton {
                    background-color: #7CB342; 
                    color: white; 
                    font-size: 18px; 
                    padding: 12px 20px; 
                    border-radius: 8px;
                }
                QPushButton:hover {
                    background-color: #689F38;
                }
            """)
            layout.addWidget(update_button, alignment=Qt.AlignCenter)

            dialog.setLayout(layout)

            # --- Handle update button click ---
            def on_update_clicked():
                dialog.accept()
                # First update the HTML button
                self.browser.page().runJavaScript("""
                    const btn = document.getElementById('update-button');
                    if (btn) {
                        // Just update label text
                        const label = btn.querySelector('.label');
                        if (label) {
                            label.textContent = 'Downloading...';
                        }

                        // Disable without removing children
                        btn.disabled = true;
                        btn.cursor= not-allowed;                                   
                        btn.classList.add('blinking-text');
                    }
                    """
                    )
                self.update_app()
                #self.update_html_button_state(callback=lambda _: self.update_app())

            update_button.clicked.connect(on_update_clicked)

            dialog.exec_()

        except Exception as e:
            logging.error(f"Error showing update dialog: {e}")

    def show_confirmation_dialog(self, parent_widget):
        dialog = QDialog(parent_widget)
        dialog.setWindowTitle("Check Out Confirmation")
        dialog.setStyleSheet("background-color: white;")
        main_layout = QVBoxLayout(dialog)
        image_label = QLabel()
        checkout_path = os.path.join( base_path , "images/checkout.png")
        person_pixmap = QPixmap(checkout_path)
        image_label.setPixmap(person_pixmap.scaled(150, 200, Qt.KeepAspectRatio, Qt.SmoothTransformation))
        image_label.setAlignment(Qt.AlignCenter)
        main_layout.addWidget(image_label)
        text_label = QLabel("Are you sure you want to checkout?")
        text_label.setAlignment(Qt.AlignCenter)
        text_label.setFont(QFont("Arial", 14))
        main_layout.addWidget(text_label)
        button_layout = QHBoxLayout()
        yes_button = QPushButton("Yes")
        yes_button.setStyleSheet("""
            QPushButton {
                background-color: #ff4d4d;
                color: white;
                border: 2px solid #ff4d4d;
                border-radius: 10px;
                padding: 10px 20px;
            }
            QPushButton:hover {
                background-color: #ff6666;
            }""")
        button_layout.addWidget(yes_button)
        main_layout.addLayout(button_layout)
        dialog.setLayout(main_layout)
        yes_button.clicked.connect(dialog.accept)

        def closeEvent(event):
            dialog.reject()
        dialog.closeEvent = closeEvent
        dialog.exec_()
        return "yes" if dialog.result() == QDialog.Accepted else "no"
        
    @pyqtSlot()
    def refresh_app(self):
        logging.info("Starting refresh...")
        self.loader_label = LoaderLabel(self.parent.root)
        self.loader_label.setVisible(True)
        self.refresh_thread = RefreshThread(self.db_manager)
        with threads_lock:
            self.threads.append(self.refresh_thread)
        self.refresh_thread.finished_signal.connect(self.on_refresh_finished)
        self.refresh_thread.start()
        
    @pyqtSlot()
    def handle_checkin(self):
        try:
            last_checkout_time = self.db_manager.get_last_checkout()
            if last_checkout_time:
                time_diff = datetime.now(pytz.utc).replace(tzinfo=None, microsecond=0) - last_checkout_time
                if time_diff < timedelta(hours=2):
                    # Show confirmation dialog
                    reply = QMessageBox.question(
                        self.parent.root, 
                        "Confirm Check-In",
                        "It's been less than 2 hours since your last check-out. Are you sure you want to check in again?",
                        QMessageBox.Yes | QMessageBox.No,
                        QMessageBox.No
                    )
                    
                    if reply == QMessageBox.No:
                        logging.warning("Check-in cancelled by user: less than 2 hours since last check-out")
                        self.enableCheckinButton()
                        self.disable_buttons()
                        return
            
            result_checkin = self.tracker.checkin()
            if not result_checkin:
                self.show_message_box(message_type='error', title="Check-In Error", text="Failed to check in. Please try again.")
                logging.error("Checkin failed")
                self.enableCheckinButton()
                self.disable_buttons()
            else:
                self.is_checked_in = True
                checkin_time_str = self.tracker.checkin_time if self.tracker.checkin_time else "--:--:--"
                logging.info(f"Checkin successful at {checkin_time_str}")
                self.disableCheckinButton()
                self.enable_buttons()
                
                self.show_message_box(message_type='info', title="Check-In Successful", text="You have successfully checked in")
                # Notify the embedded JS to start the elapsed timer immediately
                try:
                    # Prefer tracker.checkin_time if available otherwise use DB value
                    raw_checkin = None
                    if getattr(self.tracker, 'checkin_time', None):
                        raw_checkin = self.tracker.checkin_time
                    else:
                        try:
                            raw_checkin = self.db_manager.get_checkin_time()
                        except Exception:
                            raw_checkin = None

                    if raw_checkin:
                        try:
                            dt = ensure_datetime(raw_checkin)
                            if dt.tzinfo is None:
                                dt = dt.replace(tzinfo=pytz.utc)
                            checkin_ms = int(dt.timestamp() * 1000)
                            payload = {
                                'checkin_ms': checkin_ms,
                                'isCheckedIn': True,
                                'isPaused': False
                            }
                            payload_json = json.dumps(payload)
                            js = f"""
                            (function() {{
                                try {{
                                    // Update app state flags before anything else
                                    if (typeof appState !== 'undefined') {{
                                        appState.stopwatchRunning = true;
                                        appState.isPaused = false;
                                        appState.isCheckedIn = true;
                                    }}
                                    localStorage.setItem('isCheckedIn','true');
                                    localStorage.setItem('isPaused','false');

                                    // Start timer if function exists
                                    if (typeof startElapsedTimerFromPython === 'function') {{
                                        startElapsedTimerFromPython({payload_json});
                                    }} else {{
                                        window.__pendingElapsedPayload = {payload_json};
                                    }}

                                    // Safely update button states
                                    if (typeof updateButtonStates === 'function') {{
                                        try {{
                                            updateButtonStates();
                                        }} catch(e) {{
                                            console.warn('updateButtonStates failed', e);
                                        }}
                                    }} else {{
                                        try {{
                                            var inBtn = document.getElementById('checkInBtn');
                                            var outBtn = document.getElementById('checkOutBtn');
                                            var pauseBtn = document.getElementById('pause-button');
                                            var meetBtn = document.getElementById('meeting-button');

                                            if(inBtn) {{ inBtn.disabled = true; inBtn.innerHTML = '<i class="fa-solid fa-play"></i>'; }}
                                            if(outBtn) {{ outBtn.disabled = false; outBtn.innerHTML = '<i class="fa-solid fa-check"></i>'; }}
                                            if(pauseBtn) pauseBtn.disabled = false;
                                            if(meetBtn) meetBtn.disabled = false;
                                        }} catch(e) {{
                                            console.warn('manual button update failed', e);
                                        }}
                                    }}

                                    console.log('✅ startElapsedTimer payload injected successfully.');
                                }} catch(e) {{
                                    console.error('start timer injection failed', e);
                                }}
                            }})();
                            """
                            try:
                                if hasattr(self, "browser") and self.browser:
                                    self.browser.page().runJavaScript(js)
                                    logging.info("✅ Injected startElapsedTimer payload + updated button states.")
                            except Exception:
                                logging.exception("Failed to inject JS for startElapsedTimer.")

                        except Exception:
                            logging.exception('Failed to prepare checkin payload for JS')
                except Exception:
                    logging.exception('Failed to notify JS about new checkin')
                
        except Exception as e:
            logging.error(f"Error in handle_checkin: {e}")
            self.show_message_box(message_type='error', title="Check-In Error", text="An error occurred during check-in")
            #self.log_activity(f"Exception during check-in", datetime.now(pytz.utc))
        finally:
            gc.collect()

    @pyqtSlot()
    def disableCheckinButton(self):
        self.update_button_states({
            'checkInBtn': {**self.BUTTON_STATES['disabled'], 'disabled': True}
        })
    
    @pyqtSlot()
    def enableCheckinButton(self):
        self.update_button_states({
            'checkInBtn': {**self.BUTTON_STATES['enabled']['checkin'], 'disabled': False}
        })
    
    @pyqtSlot()
    def enable_buttons(self):
        self.update_button_states({
            'checkOutBtn': {**self.BUTTON_STATES['enabled']['checkout'], 'disabled': False},
            'pause-button': {**self.BUTTON_STATES['enabled']['pause'], 'disabled': False},
            'meeting-button': {**self.BUTTON_STATES['enabled']['meeting'], 'disabled': False}
        })
    
    @pyqtSlot()
    def disable_buttons(self):
        self.update_button_states({
            'checkOutBtn': {**self.BUTTON_STATES['disabled'], 'disabled': True},
            'pause-button': {**self.BUTTON_STATES['disabled'], 'disabled': True},
            'meeting-button': {**self.BUTTON_STATES['disabled'], 'disabled': True}
        })
        
    @pyqtSlot()
    def handle_checkout(self):
        logging.info("Handle_checkout slot called")
        if self.is_paused:
            msg = QMessageBox(self.parent.root)
            msg.setIcon(QMessageBox.Warning)
            msg.setWindowTitle("Check-Out Error")
            msg.setText("You cannot check out while paused. Please resume first.")
            msg.setStandardButtons(QMessageBox.Ok)
            msg.setWindowFlags(msg.windowFlags() & ~Qt.WindowCloseButtonHint | Qt.WindowStaysOnTopHint)
            ok_button = msg.button(QMessageBox.Ok)
            def on_ok_clicked():
                self.resume_activity()
            ok_button.clicked.connect(on_ok_clicked)
            msg.exec_()
            logging.warning("Checkout blocked: user is paused.")
            return
        if not self.tracker.checkin_time:
            logging.warning("Checkout blocked: no checkin time set.")
            self.show_message_box(message_type="warning", title="Check-Out Error", text="You have not checked in yet.")
            return
        self.checkout_button_clicked = True
        confirm_checkout = self.show_confirmation_dialog(self.parent.root)
        if confirm_checkout == "no":
            self.disableCheckinButton()
            self.enable_buttons()
            # Restore elapsed timer after cancelling checkout
            try:
                raw_checkin = self.tracker.checkin_time if self.tracker.checkin_time else self.db_manager.get_checkin_time()
                if raw_checkin:
                    dt = ensure_datetime(raw_checkin)
                    if dt.tzinfo is None:
                        dt = dt.replace(tzinfo=pytz.utc)
                    checkin_ms = int(dt.timestamp() * 1000)
                    payload = {
                        'checkin_ms': checkin_ms,
                        'isCheckedIn': True,
                        'isPaused': self.is_paused
                    }
                    payload_json = json.dumps(payload)
                    js = f"""
                    (function waitAndStart() {{
                        try {{
                            if (typeof startElapsedTimerFromPython === 'function') {{
                                startElapsedTimerFromPython({payload_json});
                                return;
                            }}
                            // Store payload on window in case scripts initialize later
                            try {{ window.__pendingElapsedPayload = {payload_json}; }} catch(e){{}}
                            // Retry after 150ms, give up after several retries handled by client
                            setTimeout(waitAndStart, 150);
                        }} catch (e) {{
                            console.error('waitAndStart error', e);
                        }}
                    }})();
                    """
                    self.browser.page().runJavaScript(js)
                    logging.info("Elapsed timer restored after cancelling checkout")
            except Exception as e:
                logging.exception('Failed to restore elapsed timer after cancelling checkout')
            return
        logging.info("Check-out requested")
        self.loader_label = LoaderLabel(self.parent.root)
        QTimer.singleShot(5000, self.update_gui_after_checkout)

    def update_gui_after_checkout(self):
        checkout_time = datetime.now(pytz.utc)
        logging.info(f"Checked out at {checkout_time}")
        self.parent.perform_checkout()
        
    @pyqtSlot()
    def handle_logs(self):
        with open(self.mail_file, 'r') as f:
            email = f.read()
        log_dir = os.path.dirname(self.log_file_path)
        self.log_files = [os.path.abspath(os.path.join(log_dir, f)) for f in os.listdir(log_dir) if os.path.isfile(os.path.join(log_dir, f)) and f.endswith('.log')]
        self.log_files.sort(key=lambda f: os.stat(f).st_mtime, reverse=True)
        self.log_files = self.log_files[:3]
        if not self.log_files:
            self.show_message_box(message_type="warning", title="No logs", text="No logs available to send!")
            return
        self.loader_label = LoaderLabel(self.parent.root)
        self.loader_label.setVisible(True)
        for file in self.log_files:
            send_log_thread = threading.Thread(target=self.send_log_to_server, args=(file, email,))
            send_log_thread.daemon = True
            send_log_thread.start()
            send_log_thread.join()
        QTimer.singleShot(5000, self.update_ui_after_logs_send)

    def send_log_to_server(self, file, email):
        try:
            with open(self.version_file, 'r') as f:
                version = f.read()
            TRIAL_BASE = self.db_manager.get_single_config("base_url")
            org_id = self.db_manager.get_org_id()
            user_id = self.db_manager.get_userid()
            response = integration.send_logs_to_server(user_id,file, email, org_id, TRIAL_BASE)
            self.responses.append(response)
        except Exception as e:
            logging.error(f"Error while sending logs to server: {e}", exc_info=True)
    
    def update_ui_after_logs_send(self):
        self.loader_label.setVisible(False)
        self.show_message_box(message_type="info", title="Logs sent", text="Logs sent to the server!")
        
    def show_message_box(self, message_type, title, text, parent=None):
        msg = QMessageBox(parent)
        if message_type.lower() == 'error':
            msg.setIcon(QMessageBox.Critical)
        elif message_type.lower() == 'warning':
            msg.setIcon(QMessageBox.Warning)
        elif message_type.lower() == 'info':
            msg.setIcon(QMessageBox.Information)
        elif message_type.lower() == 'question':
            msg.setIcon(QMessageBox.Question)
        else:
            raise ValueError("Invalid message_type. Use 'error', 'warning', 'info', or 'question'.")
        msg.setWindowTitle(title)
        msg.setText(text)
        msg.exec_()
        gc.collect()

    @pyqtSlot(bool)
    def on_refresh_finished(self, success):
        self.loader_label.setVisible(False)
        self.loader_label.cleanup()
        if hasattr(self, "refresh_thread") and self.refresh_thread is not None:
            self.refresh_thread.quit()
            self.refresh_thread.wait()
            self.refresh_thread.deleteLater()
            with threads_lock:
                if self.refresh_thread in self.threads:
                    self.threads.remove(self.refresh_thread)
            self.refresh_thread = None
        if success:
            logging.info("Data sent to API successfully after refresh.")
            self.show_message_box("info", "Success", "Data sent to API successfully.")
        else:
            logging.error("Failed to send data to API after refresh.")
            self.show_message_box("error", "Error", "Failed to send data to API.")
        gc.collect()

class UserActivityGUI:
    def __init__(self, app, monitor, tracker, log_file, mail_file, version_file, unique_file, db_manager,checkout_event):
        super().__init__()
        # Executor for running background tasks without creating many QThreads
        self._executor = ThreadPoolExecutor(max_workers=4)
        QCoreApplication.setAttribute(Qt.AA_UseSoftwareOpenGL)
        self.app = app
        self.monitor = monitor
        self.tracker = tracker
        self.log_file_path = log_file
        self.mail_file = mail_file
        self.version_file = version_file
        self.unique_file = unique_file
        self.db_manager = db_manager
        self.checkout_event = checkout_event
        #Initialize state variables
        self.current_dialog = None
        self._threads: List[QThread] = []
        self._animations: List[QPropertyAnimation] = []  # Track animations
        self._weak_refs: Dict[str, weakref.ref] = {}
        self.spinner_container = None
        self.spinner_movie = None
        self.org_verified = False
        with open(self.mail_file, "r") as f:
            self.email = f.read().strip()
        with open(self.version_file, "r") as f:
            self.version = f.read().strip()
        self.api_cli = ProductivityAPIClient(self.db_manager.get_single_config("base_url"), self.db_manager.get_userid(), self.db_manager.get_org_id())
        #Setup main window
        self.root = QMainWindow()
        self.root.setWindowFlags(Qt.WindowMinimizeButtonHint | Qt.CustomizeWindowHint | Qt.WindowTitleHint)
        self.root.setFixedSize(800, 580)
        self.root.setWindowTitle("Employee Productivity & Insight Console - EPIC")
        self.root.setWindowIcon(QIcon(os.path.join(base_path, "images", "icon.png")))
        self.root.closeEvent = self.custom_close_event
        # System tray setup
        icon_path = os.path.join(base_path, "images/tab.png")
        self.tray_icon = QSystemTrayIcon(QIcon(icon_path), self.root)
        self.tray_icon.setToolTip("EPIC - Employee Productivity & Insight Console")
        tray_menu = QMenu()
        restore_action = QAction("Restore", self.root)
        restore_action.triggered.connect(self.restore_window)
        tray_menu.addAction(restore_action)
        self.tray_icon.setContextMenu(tray_menu)
        self.tray_icon.activated.connect(self.on_tray_icon_activated)
        self.tray_icon.show()
        #Initialize backend
        self.browser = QWebEngineView()
        self.backend = LogActivity(self.tracker, self.browser, self, self.log_file_path,self.mail_file,self.version_file,self.db_manager, self.api_cli)
        #Initialize UI
        self._initialize_ui()

    def run(self):
        """Run the GUI application."""
        try:
            self.root.show()
            self.app.exec_()
        except Exception as e:
            logging.error(f"Error running application: {e}", exc_info=True)
        finally:
            self._cleanup_and_exit()

    def custom_close_event(self, event):
        """Handle window close event."""
        event.ignore()
        self.root.hide()
        
    def _cleanup_and_exit(self):
        """Clean up resources and exit application."""
        self._cleanup_threads()
        self._cleanup_animations()
        if hasattr(self, 'root') and self.root:
            self.root.close()
        gc.collect()
        sys.exit(1)
        
    def show_notification(self, title, message):
        """Show GUI notification"""
        msg_box = QMessageBox(self.root)
        msg_box.setText(message)
        msg_box.setWindowTitle(title)
        msg_box.setStandardButtons(QMessageBox.Ok)
        msg_box.exec_()
        msg_box.deleteLater()
        gc.collect()

    def _initialize_ui(self):
        """Initialize the UI based on configuration state."""
        try:
            if not self.db_manager.get_org_id():
                #print("No organization ID found. Prompting for org ID.")
                self.create_org_screen()
            else:
                if not self.email:
                    self.org_id = self.db_manager.get_org_id()
                    self.create_email_screen(self.org_id)
                else:
                    self.create_main_ui()
            self.root.show()
        except Exception as e:
            logging.error(f"UI initialization failed: {e}", exc_info=True)
            self._cleanup_and_exit()

    @log_function_call  
    def auto_signout(self):
        """Signout user automatically"""
        try:
            if self.backend.is_checked_in:
                logging.info("User is checked in, performing auto-checkout")
                self.perform_checkout_exipiration()
            self.clear_user_session()
            QMessageBox.warning(self.root, "Auto Sign-Out", "Your session has expired or is no longer valid. You have been signed out.")
            self._cleanup_and_exit()
        except Exception as e:
            logging.error(f"Error during auto sign-out: {e}", exc_info=True)
            QMessageBox.critical(self.root, "Sign-Out Error", f"An error occurred during sign-out: {str(e)}")
            self._cleanup_and_exit()

    def clear_user_session(self) -> None:
        """Clear user session data."""
        try:
            with open(self.mail_file, "w") as f:
                f.write("")
            logging.info("Cleared database session data")
        except Exception as e:
            logging.error(f"Error clearing user session: {e}", exc_info=True)
            raise
        finally:
            gc.collect()
        
    def show_update_dialog(self):
        """Show update dialog"""
        if hasattr(self, "update_dialog") and self.update_dialog and self.update_dialog.isVisible():
            logging.info("Update dialog is already open.")
            return
        self.update_dialog = UpdateDialog()
        self._weak_refs['update_dialog'] = weakref.ref(self.update_dialog)
        logging.info("Connecting updateRequested signal to backend")
        self.update_dialog.updateRequested.connect(self.backend.update_app)
        logging.info("Showing update dialog")
        result = self.update_dialog.exec_()
        logging.info(f"Update dialog closed with result: {result}")
        self.update_dialog.deleteLater()
        self._weak_refs.pop('update_dialog', None)
        gc.collect()
        
    def _cleanup_threads(self):
        """Clean up all tracked threads."""
        for thread in self._threads[:]:
            if thread.isRunning():
                thread.quit()
                thread.wait(500)
            thread.deleteLater()
            self._threads.remove(thread)
        # Shutdown executor if present
        try:
            if hasattr(self, '_executor') and self._executor:
                self._executor.shutdown(wait=False)
        except Exception:
            pass
        gc.collect()
        
    def _cleanup_animations(self):
        """Clean up all tracked animations."""
        for anim in self._animations[:]:
            anim.stop()
            anim.deleteLater()
            self._animations.remove(anim)
        gc.collect()
        
    def create_org_screen(self):
        """Create organization setup screen"""
        try:
            self.org_dialog = QDialog()
            self._weak_refs['org_dialog'] = weakref.ref(self.org_dialog)
            self.org_dialog.setWindowTitle("EPIC")
            self.org_dialog.setFixedSize(376, 395)
            self.org_dialog.setWindowFlags(Qt.Dialog | Qt.WindowStaysOnTopHint | Qt.WindowMinimizeButtonHint | Qt.WindowCloseButtonHint)
            self.org_dialog.setModal(True)
            icon_path = os.path.join(base_path, "images", "icon.png")
            if os.path.exists(icon_path):
                pixmap = QPixmap(icon_path)
                pixmap = pixmap.scaled(32, 32, Qt.KeepAspectRatio, Qt.SmoothTransformation)
                self.org_dialog.setWindowIcon(QIcon(pixmap))
            else:
                logging.info(f"Icon file not found: {icon_path}")
            self.org_dialog.setWindowFlags(self.org_dialog.windowFlags() & ~Qt.WindowContextHelpButtonHint)
            
            def handle_close_event(event):
                os._exit(0)
            self.org_dialog.closeEvent = handle_close_event
            screen = QApplication.primaryScreen().geometry()
            self.org_dialog.move((screen.width() - self.org_dialog.width()) // 2,(screen.height() - self.org_dialog.height()) // 2)
            layout = QVBoxLayout()
            layout.setContentsMargins(20, 20, 20, 20)
            layout.setSpacing(15)
            logo_label = QLabel()
            logo_path = os.path.join(base_path, "images/search.png")
            if os.path.exists(logo_path):
                pixmap = QPixmap(logo_path)
                logo_label.setPixmap(pixmap.scaled(120, 120, Qt.KeepAspectRatio, Qt.SmoothTransformation))
            else:
                logo_label.setText("Organization Setup")
            logo_label.setAlignment(Qt.AlignCenter)
            layout.addWidget(logo_label)
            title = QLabel("Enter your organization ID")
            title.setAlignment(Qt.AlignCenter)
            title.setFont(QFont("Arial", 12, QFont.Bold))
            layout.addWidget(title)
            input_layout = QHBoxLayout()
            icon_label = QLabel()
            icon_path = os.path.join(base_path, "images/organization.png")
            icon_pixmap = QPixmap(icon_path).scaled(20, 20, Qt.KeepAspectRatio, Qt.SmoothTransformation)
            icon_label.setPixmap(icon_pixmap)
            icon_label.setFixedWidth(25)
            icon_label.setAlignment(Qt.AlignCenter | Qt.AlignVCenter)
            self.org_input = QLineEdit()
            self.org_input.setPlaceholderText("ORG123")
            self.org_input.setFixedHeight(30)
            self.org_input.setStyleSheet("""
                QLineEdit {
                    font-size: 13px;
                    padding-left: 6px;
                    border: 1px solid #ccc;
                    border-radius: 4px;
                }""")
            input_layout.addWidget(icon_label)
            input_layout.addWidget(self.org_input)
            layout.addLayout(input_layout)
            self.warning_label = QLabel("")
            self.warning_label.setStyleSheet("color: red; font-size: 11px;")
            self.warning_label.setAlignment(Qt.AlignCenter)
            layout.addWidget(self.warning_label)
            self.submit_btn = QPushButton("Submit")
            self.submit_btn.setFixedSize(90, 30)
            self.submit_btn.setStyleSheet("""
                QPushButton {
                    background-color: #4CAF50;
                    color: white;
                    border: none;
                    border-radius: 4px;
                    font-size: 14px;
                }
                QPushButton:hover {
                    background-color: #45a049;
                }""")
            layout.addWidget(self.submit_btn, alignment=Qt.AlignCenter)
            footer_layout = QVBoxLayout()
            footer_layout.setSpacing(5)
            footer_label = QLabel("Powered by")
            footer_label.setStyleSheet("font-size: 10px; color: #888;")
            footer_label.setAlignment(Qt.AlignCenter)
            footer_layout.addWidget(footer_label)
            footer_logo = QLabel()
            footer_logo_pixmap = QPixmap(os.path.join(base_path, "images/logo.png")).scaled(80, 65, Qt.KeepAspectRatio)
            footer_logo.setPixmap(footer_logo_pixmap)
            footer_logo.setAlignment(Qt.AlignCenter)
            footer_layout.addWidget(footer_logo)
            version_label = QLabel(f"Version: {self.version}")
            version_label.setStyleSheet("font-size: 9px; color: #888;")
            version_label.setAlignment(Qt.AlignCenter)
            footer_layout.addWidget(version_label)
            layout.addLayout(footer_layout)
            self.org_dialog.setLayout(layout)
            self.submit_btn.clicked.connect(self.on_submit)
            self.org_input.returnPressed.connect(self.on_submit)
            self.org_input.setFocus()
            result = self.org_dialog.exec_()
            return result == QDialog.Accepted and self.org_verified
        except Exception as e:
            logging.error(f"Error creating org screen: {e}", exc_info=True)
            QMessageBox.critical(None, "Error", f"Failed to load organization screen: {str(e)}")
            self._cleanup_and_exit()
            return False
        finally:
            gc.collect()

    def create_email_screen(self, org_id):
        """Create email login screen"""
        email_dialog = QDialog()
        self._weak_refs['email_dialog'] = weakref.ref(email_dialog)
        email_dialog.setFixedSize(375, 500)
        email_dialog.setWindowTitle("Employee Productivity & Insight Console")
        icon_path = os.path.join(base_path, "images", "icon.png")
        if os.path.exists(icon_path):
            pixmap = QPixmap(icon_path)
            pixmap = pixmap.scaled(32, 32, Qt.KeepAspectRatio, Qt.SmoothTransformation)
            email_dialog.setWindowIcon(QIcon(pixmap))
        else:
            logging.info(f"Icon file not found: {icon_path}")
        email_dialog.setWindowFlags(Qt.Window | Qt.WindowTitleHint | Qt.CustomizeWindowHint | Qt.WindowCloseButtonHint)
        main_layout = QVBoxLayout(email_dialog)
        main_layout.setContentsMargins(15, 15, 15, 15)
        main_layout.setSpacing(10) 
        logo_label = QLabel()
        logo_path = os.path.join(base_path, "images/icon.png")
        logo_pixmap = QPixmap(logo_path).scaled(140, 140, Qt.KeepAspectRatio)
        logo_label.setPixmap(logo_pixmap)
        logo_label.setAlignment(Qt.AlignCenter)
        main_layout.addWidget(logo_label)
        title_label = QLabel("Let's Get Started!")
        title_label.setAlignment(Qt.AlignCenter)
        title_label.setFont(QFont("Arial", 18, QFont.Bold))
        title_label.setStyleSheet("color: #333333; margin-bottom: 10px;")
        main_layout.addWidget(title_label)
        form_layout = QFormLayout()
        form_layout.setSpacing(10)
        email_layout = QHBoxLayout()
        email_icon = QLabel()
        email_icon_pixmap = QPixmap(os.path.join(base_path, "images/email.png")).scaled(20, 20, Qt.KeepAspectRatio)
        email_icon.setPixmap(email_icon_pixmap)
        email_icon.setFixedWidth(25)
        email_icon.setAlignment(Qt.AlignCenter | Qt.AlignVCenter)
        email_input = QLineEdit()
        email_input.setPlaceholderText("Enter you e-mail")
        email_input.setFixedHeight(30)
        email_input.setStyleSheet("""
            QLineEdit {
                font-size: 13px;
                padding-left: 6px;
                border: 1px solid #ccc;
                border-radius: 4px;
            }""")
        email_layout.addWidget(email_icon)
        email_layout.addWidget(email_input)
        email_layout.setSpacing(8)
        form_layout.addRow(email_layout)
        password_layout = QHBoxLayout()
        password_icon = QLabel()
        password_icon_pixmap = QPixmap(os.path.join(base_path, "images/lock.png")).scaled(20, 20, Qt.KeepAspectRatio)
        password_icon.setPixmap(password_icon_pixmap)
        password_icon.setFixedWidth(25)
        password_icon.setAlignment(Qt.AlignCenter | Qt.AlignVCenter)
        password_input = QLineEdit()
        password_input.setPlaceholderText("Enter your password")
        password_input.setEchoMode(QLineEdit.Password)
        password_input.setFixedHeight(30)
        password_input.setStyleSheet("""
            QLineEdit {
                font-size: 13px;
                padding-right: 30px;
                padding-left: 6px;
                border: 1px solid #ccc;
                border-radius: 4px;
            }""") 
        see_password_button = QToolButton(password_input)
        see_password_button.setCursor(Qt.PointingHandCursor)
        see_password_button.setStyleSheet("border: none; padding: 0px;")
        see_password_button.setIcon(QIcon(os.path.join(base_path, "images/eye.png")))
        see_password_button.setIconSize(QSize(18, 18))
        see_password_button.setCheckable(True)
        see_password_button.setFixedSize(20, 20)
        see_password_button.move(password_input.rect().right() - 25, 5)
        eye_open = QIcon(os.path.join(base_path, "images/eye.png"))
        eye_closed = QIcon(os.path.join(base_path, "images/eye-slash.png"))
    
        def toggle_password_visibility():
            if see_password_button.isChecked():
                password_input.setEchoMode(QLineEdit.Normal)
                see_password_button.setIcon(eye_closed)
            else:
                password_input.setEchoMode(QLineEdit.Password)
                see_password_button.setIcon(eye_open)
        
        see_password_button.clicked.connect(toggle_password_visibility)
        
        def adjust_eye_position(event):
            see_password_button.move(
                password_input.width() - see_password_button.width() - 5,
                (password_input.height() - see_password_button.height()) // 2
            )
            QLineEdit.resizeEvent(password_input, event)
        
        password_input.resizeEvent = adjust_eye_position
        password_layout.addWidget(password_icon)
        password_layout.addWidget(password_input)
        password_layout.setSpacing(8)
        form_layout.addRow(password_layout)
        warning_label = QLabel("")
        warning_label.setStyleSheet("color: red; font-size: 11px; margin-top: 5px;")
        form_layout.addRow(warning_label)
        submit_button = QPushButton("Submit")
        submit_button.setFixedSize(90, 30)
        submit_button.setStyleSheet("""
            QPushButton {
                background-color: #4CAF50;
                color: white;
                border: none;
                border-radius: 4px;
                font-size: 14px;}
            QPushButton:hover {
                background-color: #45a049;
            }""")
        button_layout = QHBoxLayout()
        button_layout.addWidget(submit_button)
        button_layout.setAlignment(Qt.AlignCenter)
        main_layout.addLayout(form_layout)
        main_layout.addLayout(button_layout)
        footer_layout = QVBoxLayout()
        footer_label = QLabel("Powered by")
        footer_label.setStyleSheet("font-size: 10px; color: #888;")
        footer_label.setAlignment(Qt.AlignCenter)
        footer_layout.addWidget(footer_label)
        footer_logo = QLabel()
        footer_logo_pixmap = QPixmap(os.path.join(base_path, "images/logo.png")).scaled(80, 65, Qt.KeepAspectRatio)
        footer_logo.setPixmap(footer_logo_pixmap)
        footer_logo.setAlignment(Qt.AlignCenter)
        footer_layout.addWidget(footer_logo)
        version_label = QLabel(f"Version: {self.version}")
        version_label.setStyleSheet("font-size: 9px; color: #888;")
        version_label.setAlignment(Qt.AlignCenter)
        footer_layout.addWidget(version_label)
        main_layout.addLayout(footer_layout)
        
        def validate():
            email = email_input.text().strip()
            pwd = password_input.text().strip()
            if not self.is_valid_email(email):
                warning_label.setText("Invalid email format.")
                submit_button.setEnabled(True)
                submit_button.setText("Submit")
                return
            if len(pwd) < 6:
                warning_label.setText("Password too short.")
                submit_button.setEnabled(True)
                submit_button.setText("Submit")
                return
            warning_label.setText("")
            self.submit_email(email, pwd, org_id, email_dialog)
        
        submit_button.clicked.connect(validate)

        def closeEvent(event):
            """Handle the close event for the email dialog."""
            email_input.clear()
            password_input.clear()
            warning_label.setText("")
            gc.collect()
            os._exit(0)
            event.accept()

        email_dialog.closeEvent = closeEvent
        email_dialog.setWindowModality(Qt.ApplicationModal)
        email_dialog.exec_()
        gc.collect()
       
    def create_main_ui(self):
        """Create the main UI screen."""
        self.control_panel = QWidget(self.root)
        self._weak_refs['control_panel'] = weakref.ref(self.control_panel)
        self.control_panel.setObjectName("control_panel")
        self.layout = QVBoxLayout(self.control_panel)
        self._weak_refs['browser'] = weakref.ref(self.browser)
        self.browser.setZoomFactor(1.0)
        self.browser.setContextMenuPolicy(Qt.NoContextMenu)
        settings = self.browser.settings()
        settings.setAttribute(QWebEngineSettings.WebGLEnabled, False)
        self.channel = QWebChannel()
        self._weak_refs['backend'] = weakref.ref(self.backend)
        self.channel.registerObject("backend", self.backend)
        logging.info('QWebChannel: registered "backend" object')
        self.browser.page().setWebChannel(self.channel)
        logging.info('QWebChannel: setWebChannel called on browser page')

        file_path = os.path.join(base_path, "index.html")
        self.browser.setUrl(QUrl.fromLocalFile(os.path.abspath(file_path)))
        icon_path = os.path.join(base_path, "images", "icon.png")
        if os.path.exists(icon_path):
            pixmap = QPixmap(icon_path)
            pixmap = pixmap.scaled(32, 32, Qt.KeepAspectRatio, Qt.SmoothTransformation)
            self.root.setWindowIcon(QIcon(pixmap))
        else:
            logging.info(f"Icon file not found: {icon_path}")
        self.browser.loadFinished.connect(self.on_load_finished)
        self.layout.addWidget(self.browser)
        self.root.setCentralWidget(self.control_panel)
        self.compare_version_files()
        #self.update_theme()
        # Start theme monitoring timer
        self.theme_timer = QTimer()
        self.theme_timer.timeout.connect(self.update_theme)
        self.theme_timer.start(5000)  # Check every 5 seconds

    def update_theme(self):
        """Update the UI theme based on system theme changes."""
        try:
            is_dark = self.detect_system_theme()
            theme_js = f"""
            if ({'true' if is_dark else 'false'}) {{
                document.body.classList.add('dark');
            }} else {{
                document.body.classList.remove('dark');
            }}
            """
            self.browser.page().runJavaScript(theme_js)
            # Apply dark stylesheet to PyQt5 window for dark theme
            if is_dark:
                self.root.setStyleSheet("""
                        QMainWindow {
                            background-color: #2e2e2e;
                            color: #ffffff;
                        }
                """)
                try:
                    import ctypes
                    hwnd = int(self.root.winId())
                    DWMWA_USE_IMMERSIVE_DARK_MODE = 20
                    ctypes.windll.dwmapi.DwmSetWindowAttribute(
                        hwnd,
                        DWMWA_USE_IMMERSIVE_DARK_MODE,
                        ctypes.byref(ctypes.c_int(1)),
                        ctypes.sizeof(ctypes.c_int)
                    )
                except Exception as e:
                    logging.error(f"Failed to set dark title bar: {e}")
            else:
                self.root.setStyleSheet("")
                # Apply light theme to window title bar on Windows
                if platform.system() == "Windows":
                    try:
                        from ctypes import windll, c_int
                        hwnd = self.root.winId()
                        DWMWA_USE_IMMERSIVE_DARK_MODE = 20
                        result = windll.dwmapi.DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, c_int(0), 4)
                        if result != 0:
                            logging.warning(f"DwmSetWindowAttribute returned non-zero: {result}")
                        else:
                            logging.info("Light title bar applied successfully")
                    except Exception as e:
                        logging.error(f"Failed to set light title bar: {e}")
            self.theme_timer.stop()
        except Exception as e:
            logging.error(f"Error updating theme: {e}")
        finally:
            gc.collect()

        if self.tracker.was_checked_in_before_shutdown:
            self.backend.handle_resume()
            self.backend.disableCheckinButton()
            self.backend.enable_buttons()
        else:
            self.backend.enableCheckinButton()
            self.backend.disable_buttons()
            self.is_checked_in = False
        
    def perform_checkout_exipiration(self):
        """Completes the auto checkout functionality silently, updates UI immediately after DB logging, sends data in background."""
        try:
            self._checkout_finalized = False
            if self.checkout_event:
                logging.info("Triggering checkout_event from GUI")
                self.checkout_event.set()
            checkout_time = datetime.now(pytz.utc)
            checkin_time = self.db_manager.get_checkin_time()
            success, total_elapsed_time = self.db_manager.log_checkout(checkin_time, checkout_time)
            # Update UI immediately after DB logging
            self._update_ui_after_auto_checkout()
            # Send data in background
            self._send_checkout_data_background()
            return True
        except Exception as e:
            logging.error(f"Auto check-out error: {e}")
            return False

    def _update_ui_after_auto_checkout(self):
        """Update UI elements after auto-checkout: reset buttons and timer silently."""
        try:
            logging.info("Updating UI after auto-checkout")
            self.backend.is_checked_in = False
            self.is_checked_in = False
            # Update button states
            self.backend.enableCheckinButton()
            self.backend.disable_buttons()
            # Reset client-side stopwatch/timer and button states
            js_reset = """
            (function(){
                try {
                    try { localStorage.setItem('isCheckedIn', 'false'); } catch(e){}
                    try { localStorage.setItem('isPaused', 'false'); } catch(e){}
                    // Reset client appState safely if present
                    if (typeof appState !== 'undefined') {
                        appState.stopwatchRunning = false;
                        appState.isPaused = false;
                    }
                    // Clear the running timer interval
                    if (typeof window.elapsedTimer !== 'undefined') {
                        clearInterval(window.elapsedTimer);
                        window.elapsedTimer = null;
                    }
                    // Prefer the primary session timer element used by the app
                    try {
                        var display = document.getElementById('sessionTimer') || document.getElementById('elapsed-timer') || document.querySelector('.stopwatch') || document.getElementById('stopwatch');
                        if (display) {
                            display.classList.remove('running');
                            // Use the placeholder the app uses when not running
                            display.textContent = '--:--:--';
                        }
                    } catch(e){}
                    try { var inBtn = document.getElementById('checkInBtn'); if(inBtn) { inBtn.disabled = false; inBtn.innerHTML = '<i class="fa-solid fa-play"></i>'; } } catch(e){}
                    try { var outBtn = document.getElementById('checkOutBtn'); if(outBtn) { outBtn.disabled = true; outBtn.innerHTML = '<i class="fa-solid fa-check"></i>'; } } catch(e){}
                    try { var pause = document.getElementById('pause-button'); if(pause) pause.disabled = true; } catch(e){}
                    try { var meet = document.getElementById('meeting-button'); if(meet) meet.disabled = true; } catch(e){}
                    // Ensure the UI's derived state is refreshed if helper functions exist
                    try { if (typeof updateButtonStates === 'function') updateButtonStates(); } catch(e){}
                    try { if (typeof updateSystemStats === 'function') updateSystemStats(); } catch(e){}
                } catch (err) { console.warn('auto-checkout UI reset failed', err); }
            })();
            """
            if hasattr(self, 'browser') and self.browser:
                try:
                    self.browser.page().runJavaScript(js_reset)
                except Exception:
                    logging.exception('Failed to run JS to reset UI after auto-checkout')
        except Exception as e:
            logging.exception('Failed to update UI after auto-checkout')
        finally:
            gc.collect()

    def _send_checkout_data_background(self):
        """Send checkout data to server in a background thread."""
        try:
            logging.info("Starting background send of checkout data")
            def send_data():
                try:
                    self.db_manager.send_checkout_data()
                    logging.info("Checkout data sent successfully in background")
                except Exception as e:
                    logging.error(f"Failed to send checkout data in background: {e}")
                finally:
                    gc.collect()
            # Use a daemon thread to avoid blocking exit
            send_thread = threading.Thread(target=send_data, daemon=True)
            send_thread.start()
        except Exception as e:
            logging.exception('Failed to start background send thread')
        finally:
            gc.collect()        
        
    def restore_window(self):
        """Restore the main window from system tray."""
        self.root.show()
        self.root.raise_()
        self.root.activateWindow()
        
    def on_tray_icon_activated(self, reason):
        """Handle system tray icon activation."""
        if reason == QSystemTrayIcon.DoubleClick:
            self.restore_window()
        
    def on_submit(self):
        """Handle submit button click for organization verification."""
        org_id = self.org_input.text().strip()
        if not org_id:
            self.warning_label.setText("Organization ID cannot be empty")
            return
        self.warning_label.setText("")
        self.org_input.setEnabled(False)
        self.submit_btn.setEnabled(False)
        QApplication.processEvents()
        self._setup_spinner()
        self.handle_submit(org_id)
      
    def submit_email(self, email, password, org_id, dialog):
        """Handle email/password submission."""
        self.email = email
        self.password = password
        self.org_id = org_id
        self.current_dialog = dialog
        self._weak_refs['current_dialog'] = weakref.ref(dialog)
        layout = dialog.layout()
        form_widgets = self._collect_form_widgets(layout)
        self.unique_key = str(uuid.uuid4())
        with open(self.unique_file, "w") as f:
            f.write(self.unique_key)
        self.api_cli = ProductivityAPIClient(self.db_manager.get_single_config("base_url"), self.db_manager.get_userid(), self.db_manager.get_org_id())
        has_logged_in = self.api_cli.check_login(self.email, self.db_manager.get_org_id())
        if has_logged_in is False:
            disclaimer_dialog = DisclaimerDialog(self.root)
            if disclaimer_dialog.exec_() == QDialog.Accepted:
                self._initiate_authentication_flow(form_widgets)
            else:
                dialog.reject()
                os._exit(0)
        else:
            self._initiate_authentication_flow(form_widgets)

    @log_function_call
    def on_load_finished(self, success):
        """Handle HTML page load completion and restore/check auto-checkout logic."""
        try:
            # ----------------------------
            # 1. Initialize QWebChannel
            # ----------------------------
            self.browser.page().setWebChannel(self.channel)
            logging.info("✅ WebChannel initialized successfully")

            # ----------------------------
            # 2. Diagnostic JS connectivity
            # ----------------------------
            try:
                test_js = """
                (function waitForBackend(){
                    try {
                        if (typeof window.backend !== 'undefined') {
                            try { window.backend.notify_elapsed_payload('page_load_ping'); } catch(e){}
                            return;
                        }
                        setTimeout(waitForBackend, 150);
                    } catch (e) {
                        console.error('waitForBackend error', e);
                        setTimeout(waitForBackend, 150);
                    }
                })();
                """
                self.browser.page().runJavaScript(test_js)
                logging.info("🧠 Diagnostic JS dispatched to verify QWebChannel connectivity.")
            except Exception:
                logging.exception("Failed to dispatch diagnostic backend call")

            # ----------------------------
            # 3. Load user data, theme, version
            # ----------------------------
            try:
                self._load_user_data()
            except Exception:
                logging.exception("Failed loading user data")

            try:
                self.browser.page().runJavaScript(
                    f"document.getElementById('version').innerText = '{self.version}'"
                )
            except Exception:
                logging.warning("Could not inject version info")

            try:
                is_dark = self.detect_system_theme()
                theme_js = f"""
                if ({'true' if is_dark else 'false'}) {{
                    document.body.classList.add('dark');
                }} else {{
                    document.body.classList.remove('dark');
                }}
                """
                self.browser.page().runJavaScript(theme_js)
            except Exception:
                logging.warning("Could not apply system theme")

            # ----------------------------
            # 4. Check DB for last check-in
            # ----------------------------
            checkin_time = None
            try:
                checkin_time = self.db_manager.get_checkin_time()
            except Exception:
                logging.warning("DB checkin lookup failed")

            if checkin_time:
                try:
                    checkin_dt = ensure_datetime(checkin_time)
                    '''if checkin_dt.tzinfo is None:
                        checkin_dt = checkin_dt.replace(tzinfo=pytz.utc)
                    else:
                        checkin_dt = checkin_dt.astimezone(pytz.utc)'''
                    if checkin_dt.tzinfo is None:
                        checkin_dt = checkin_dt.replace(tzinfo=pytz.UTC)
                    now_utc = datetime.now(pytz.utc)
                    diff = now_utc - checkin_dt
                except Exception:
                    logging.exception("Error parsing checkin time")
                    diff = timedelta(0)
            else:
                diff = timedelta(0)

            # ----------------------------
            # 5. Auto-checkout if >24h
            # ----------------------------
            if checkin_time and diff > timedelta(hours=24):
                logging.info(f"🕒 Auto-checkout triggered (last checkin {diff} ago).")

                checkout_time = (checkin_dt + timedelta(hours=24)).replace(tzinfo=None, microsecond=0)

                # Step 1: End all active breaks
                try:
                    self.db_manager.end_all_breaks(checkout_time, checkin_dt)
                    logging.info("All active breaks closed before auto-checkout.")
                except Exception:
                    logging.exception("Failed to end all breaks during auto-checkout")

                # Step 2: Log checkout in DB
                try:
                    naive_checkin = checkin_dt.replace(tzinfo=None, microsecond=0)
                    success, total_elapsed = self.db_manager.log_checkout(naive_checkin, checkout_time)
                    logging.info(f"Auto-checkout logged successfully: {success}, total_elapsed={total_elapsed}")
                except Exception:
                    logging.exception("Failed to log auto-checkout in DB")

                # Step 3: Reset UI immediately after DB logging (silent, no popups/loaders)
                self.reset_ui_after_checkout()

                # Step 4: Send checkout data in background
                self._send_checkout_data_background()

                logging.info("✅ Auto-checkout: DB logged, UI reset immediate, data sending in background")
                return  # ✅ stop flow here after auto-checkout

            # ----------------------------
            # 6. Resume timer if still within 24 hours
            # ----------------------------
            if checkin_time and diff <= timedelta(hours=24):
                logging.info(f"Resuming session (checked in {diff} ago)")
                try:
                    payload = None
                    checkin_source = None
                    raw_checkin = None
                    if getattr(self, 'tracker', None) and getattr(self.tracker, 'checkin_time', None):
                        raw_checkin = self.tracker.checkin_time
                        checkin_source = 'tracker'
                    else:
                        try:
                            raw_checkin = self.db_manager.get_checkin_time()
                            checkin_source = 'db'
                        except Exception:
                            raw_checkin = None
                            checkin_source = None
                    if raw_checkin:
                        try:
                            dt = ensure_datetime(raw_checkin)
                            if dt.tzinfo is None:
                                dt = dt.replace(tzinfo=pytz.utc)
                            checkin_ms = int(dt.timestamp() * 1000)
                            payload = {
                                'checkin_ms': checkin_ms,
                                'isCheckedIn': bool(getattr(self, 'is_checked_in', False)) or (checkin_source == 'db'),
                                'isPaused': bool(getattr(self, 'is_paused', False))
                            }
                            logging.debug(f"Resuming elapsed timer from {checkin_source} checkin: {dt} ({checkin_ms} ms)")
                        except Exception:
                            logging.exception('Failed to parse raw_checkin for elapsed timer')
                            payload = None
                    payload_json = json.dumps(payload) if payload is not None else 'null'
                    js = f"""
                    (function waitAndStart() {{
                        try {{
                            if (typeof startElapsedTimerFromPython === 'function') {{
                                startElapsedTimerFromPython({payload_json});
                                return;
                            }}
                            // Store payload on window in case scripts initialize later
                            try {{ window.__pendingElapsedPayload = {payload_json}; }} catch(e){{}}
                            // Retry after 150ms, give up after several retries handled by client
                            setTimeout(waitAndStart, 150);
                        }} catch (e) {{
                            console.error('waitAndStart error', e);
                        }}
                    }})();
                    """
                    self.browser.page().runJavaScript(js)
                except Exception:
                    logging.exception('Failed to trigger client-side elapsed timer on load')

            else:
                # ----------------------------
                # 7. No check-in: clean default UI
                # ----------------------------
                logging.info("No active check-in found. Resetting UI to default state.")
                QTimer.singleShot(1000, lambda: self.reset_ui_after_checkout())

            # ----------------------------
            # 8. Background startup tasks
            # ----------------------------
            QTimer.singleShot(500, self._save_connection_details)
            QTimer.singleShot(600, self._start_location_tracking)
            QTimer.singleShot(700, self._schedule_tasks)

            def _submit_bg(task_func, callback=None):
                try:
                    future = self._executor.submit(task_func)
                    if callback:
                        def _done(f):
                            try:
                                result = f.result()
                            except Exception as e:
                                logging.error(f"Background task {task_func.__name__} failed: {e}", exc_info=True)
                                result = None
                            QTimer.singleShot(0, lambda r=result: callback(r))
                        future.add_done_callback(_done)
                    return future
                except Exception:
                    logging.exception(f"Failed to submit background task: {task_func}")
                    return None

            _submit_bg(self._sync_with_server, callback=self._on_sync_finished)
            _submit_bg(self._device_primary_check)
            _submit_bg(self._check_employee_status)
            QTimer.singleShot(0, self._validity_check)

        except Exception:
            logging.exception("Critical failure in on_load_finished")
            raise

    def reset_ui_after_checkout(self):
        """Reset UI elements and JS timer after checkout."""
        try:
            self.stop_timer()  # ensure all timers halted

            # Reset JS-based UI (HTML buttons, display, etc.)
            js_reset = """
            (function(){
                try {
                    localStorage.setItem('isCheckedIn','false');
                    localStorage.setItem('isPaused','false');
                    const display = document.getElementById('sessionTimer');
                    if(display){ display.textContent='--:--:--'; display.classList.remove('running'); }

                    const inBtn = document.getElementById('checkInBtn');
                    const outBtn = document.getElementById('checkOutBtn');
                    const pause = document.getElementById('pause-button');
                    const meet = document.getElementById('meeting-button');

                    if(inBtn){ inBtn.disabled = false; inBtn.innerHTML = '<i class="fa-solid fa-play"></i>'; }
                    if(outBtn){ outBtn.disabled = true; outBtn.innerHTML = '<i class="fa-solid fa-check"></i>'; }
                    if(pause) pause.disabled = true;
                    if(meet) meet.disabled = true;

                    // Stop the elapsed timer interval
                    if (typeof window.elapsedTimer !== 'undefined') {
                        clearInterval(window.elapsedTimer);
                        window.elapsedTimer = null;
                    }

                    if(typeof updateButtonStates === 'function') updateButtonStates();
                    if(typeof updateSystemStats === 'function') updateSystemStats();

                    console.log('✅ UI fully reset after checkout.');
                } catch(e) {
                    console.warn('reset_ui_after_checkout failed', e);
                }
            })();
            """
            self.browser.page().runJavaScript(js_reset)

            # Reset backend flags
            self.backend.is_checked_in = False
            self.is_checked_in = False
            self.is_paused = False
            if hasattr(self, "tracker"):
                self.tracker.checkin_time = None
                self.tracker.elapsed_seconds = 0

            # Update backend controls
            self.backend.enableCheckinButton()
            self.backend.disable_buttons()

            logging.info("✅ UI reset complete (JS + Python state cleared).")

        except Exception as e:
            logging.error(f"Error resetting UI: {e}")

    

    def stop_timer(self):
        """Stops the running timer and resets related state."""
        try:
            if hasattr(self, 'timer') and self.timer.isActive():
                self.timer.stop()
                logging.info("🛑 Timer stopped successfully.")
            else:
                logging.info("Timer was not active or not initialized.")
        except Exception as e:
            logging.error(f"Error while stopping timer: {e}")
        finally:
            gc.collect()




    def _run_asyncio_loop(self):
        """Run the asyncio event loop in a separate thread."""
        asyncio.set_event_loop(self._asyncio_loop)
        try:
            self._asyncio_loop.run_forever()
        except Exception as e:
            logging.error(f"Error in asyncio loop: {e}", exc_info=True)
        finally:
            self._asyncio_loop.run_until_complete(self._asyncio_loop.shutdown_asyncgens())
            self._asyncio_loop.close()

    async def _run_delayed_task(self, delay: float, task_func):
        """Run a task after a specified delay, handling sync and async tasks."""
        try:
            await asyncio.sleep(delay)
            # If the task is a GUI function, always schedule on the main thread
            if task_func in [self._validity_check, self._device_primary_check, self._check_employee_status]:
                QTimer.singleShot(0, task_func)
            elif asyncio.iscoroutinefunction(task_func):
                await task_func()
            else:
                loop = asyncio.get_event_loop()
                await loop.run_in_executor(None, task_func)
        except Exception as e:
            logging.error(f"Error in task {getattr(task_func, '__name__', str(task_func))}: {e}", exc_info=True)

    @log_function_call
    def detect_system_theme(self):
        """Detect system theme using Windows registry."""
        try:
            settings = QSettings("HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize", QSettings.NativeFormat)
            use_light_theme = settings.value("AppsUseLightTheme", 1)  # Default to 1 (light theme)
            return use_light_theme == 0  # True if dark theme
        except Exception as e:
            logging.warning(f"Failed to detect system theme: {e}")
            return False  # Default to light theme
        finally:
            gc.collect()

    def compare_version_files(self):
        """Compare version files."""
        try:
            current_version = self.db_manager.get_version_()
            if self.version > current_version:
                self.db_manager.update_version(self.version)
                logging.info("Updated version in database!")
            else:
                logging.info("Both version files have the same version!")
        except Exception as e:
            logging.error(f"Error comparing version files: {e}", exc_info=True)
        finally:
            gc.collect()
        
    def _setup_spinner(self):
        """Clear existing spinner if it exists"""
        if self.spinner_container:
            if self.spinner_container.isVisible():
                self.spinner_container.hide()
            self.spinner_container.deleteLater()
            self._weak_refs.pop('spinner_container', None)
            self.spinner_container = None
        if self.spinner_movie:
            self.spinner_movie.stop()
            self.spinner_movie.deleteLater()
            self._weak_refs.pop('spinner_movie', None)
            self.spinner_movie = None

        # Create new spinner
        self.spinner_container, self.spinner_movie = self.show_spinner_overlay(self.org_dialog, base_path)
        self._weak_refs['spinner_container'] = weakref.ref(self.spinner_container)
        self._weak_refs['spinner_movie'] = weakref.ref(self.spinner_movie)

    def handle_submit(self, org_id):
        """Handle organization ID submission."""
        QTimer.singleShot(1500, lambda: self.verify_organization(org_id))
        
    def _initiate_authentication_flow(self, form_widgets):
        """Start authentication process."""
        def on_fade_out_done():
            self._show_spinner_and_authenticate(form_widgets)
        self.fade_widgets(form_widgets, 1, 0, 600, on_fade_out_done)
        
    def _collect_form_widgets(self, layout):
        """Collect form widgets for fading."""
        form_widgets = []
        for i in range(layout.count()):
            item = layout.itemAt(i)
            widget = item.widget() if item else None
            if widget and isinstance(widget, (QLineEdit, QPushButton, QLabel)):
                if isinstance(widget, QLabel) and (widget.pixmap() or len(widget.text()) > 20):
                    continue
                form_widgets.append(widget)
        return form_widgets
        
    def _load_user_data(self):
        self.user_data = self.db_manager.get_user_data()
        if not self.user_data:
            raise ValueError("No user data found in local database")
        self.user_data = json.loads(self.user_data)
        self.org_id = self.db_manager.get_org_id()
        if not self.org_id:
            raise ValueError("Organization ID not found in user data")

    def clean_up_old_logs(self, seven_days: datetime) -> None:
        """Clean up logs older than 7 days."""
        log_dir = os.path.dirname(self.log_file_path)
        if os.path.exists(self.log_file_path):
            log_created = datetime.fromtimestamp(os.path.getctime(self.log_file_path))
            if (datetime.now() - log_created) < timedelta(minutes=10):
                logging.info("Log file is very new, skipping cleanup (first run).")
                return
        
        files = [
            os.path.abspath(os.path.join(log_dir, f)) for f in os.listdir(log_dir)
            if os.path.isfile(os.path.join(log_dir, f))
            and f not in [
                "keys.key", "user_inactivity.db", "email_detail.txt",
                "unique.txt", "last_cleanup_timestamp.txt", "version_file.txt", "shutdown.txt"
            ]
        ]
        
        for file in files:
            file_path = os.path.join(self.log_file_path, file)
            if os.path.isfile(file_path):
                file_mod_time = datetime.fromtimestamp(os.path.getmtime(file_path))
                if file_mod_time < seven_days:
                    os.remove(file_path)
                    logging.info(f"Removed old log file: {file_path}")
        
        screenshots_dir = os.path.join(log_dir, "screenshots")
        if os.path.exists(screenshots_dir):
            for file in os.listdir(screenshots_dir):
                file_path = os.path.join(screenshots_dir, file)
                if os.path.isfile(file_path):
                    file_mod_time = datetime.fromtimestamp(os.path.getmtime(file_path))
                    if file_mod_time < seven_days:
                        os.remove(file_path)
                        logging.info(f"Removed old screenshot: {file_path}")
        gc.collect()    
    
    @log_function_call
    def _sync_with_server(self):
        logging.info("Starting _sync_with_server background task")
        try:
            self.api_cli = ProductivityAPIClient(self.db_manager.get_single_config("base_url"), self.db_manager.get_userid(), self.db_manager.get_org_id())
            server_data = self.api_cli.get_user_data(self.email, self.org_id)
            if not server_data:
                QMessageBox.warning(self.root, "No User Data", "No user data found on the server. Please check your validity or contact support.")
                self.auto_signout()
                raise ValueError("No user data found on the server")
            if self.compare_and_sync(server_data, self.user_data):
                self.user_data = self.db_manager.fetch_and_log_user_data(server_data, self.org_id)
            try:
                conn = self.get_connection_type() or 'Unknown'
            except Exception:
                conn = 'Unknown'
            server_data['connection_type'] = conn
            server_data['connType'] = conn
            try:
                ip_addr = self.get_public_ip() or "Unavailable"
            except Exception:
                ip_addr = "Unavailable"
            server_data['ip_address'] = ip_addr
            try:
                location = None
                if hasattr(self, 'backend') and getattr(self.backend, 'location', None):
                    location = getattr(self.backend, 'location')
                if not location:
                    location ="{}, {}".format(*[requests.get("https://ipapi.co/json/").json().get(k, "Unknown") for k in ("city", "country_name")])
            except Exception:
                location = 'Unknown'
            server_data['location'] = location
            # Return server_data to the caller so UI-updating code runs on the main thread
            self.inject_js_functions(server_data)
            return server_data
        except Exception as e:
            logging.warning(f"Unable to sync user data with server. Proceeding with local DB. Error: {e}", exc_info=True)
            self.is_offline_mode = True
            return None

    def is_valid_email(self, email: str) -> bool:
        """Check if email is valid."""
        return re.match(r"[^@]+@[^@]+\.[^@]+", email) is not None

    def _restore_ui(self, form_widgets: List[QWidget]) -> None:
        """Restore UI elements after failed attempt."""
        for w in form_widgets:
            w.show()
        self.fade_widgets(form_widgets, 0, 1, 600)

    @log_function_call 
    def _device_primary_check(self):
        """Check if current device is set as primary"""
        try:
            hostname = socket.gethostname()
            os_name = platform.system()
            current_device_info = {"selected_device_id": hostname, "os_name": os_name}
            user_id = self.db_manager.get_userid()
            self.api_cli = ProductivityAPIClient(self.db_manager.get_single_config("base_url"), self.db_manager.get_userid(), self.db_manager.get_org_id())
            devices = self.api_cli.check_primary_device(self.email, self.org_id, user_id)
            if not devices or "primary_device_id" not in devices:
                self.api_cli.register_device(user_id, self.email, self.org_id, current_device_info)
            elif hostname != devices.get("primary_device_id"):
                #QMessageBox.warning(self.root, "Access Denied", "This device is not set as your primary device. You will be signed out.")
                #self.auto_signout()
                self.api_cli.set_primary_device(user_id, self.email, self.org_id, current_device_info)
                return
        except requests.exceptions.RequestException:
            logging.warning("No internet. Assuming current device is primary.")
            self.device_is_primary = True
            self.is_offline_mode = True
        except Exception as e:
            logging.error(f"Device primary check failed: {e}", exc_info=True)
            self.device_is_primary = True
            self.is_offline_mode = True

    @log_function_call  
    def _check_employee_status(self):
        """Check if employee is active"""
        self.check_employee_status()

    def _resume_or_checkin_flow(self):
        """If checked in, resume. If not, enable check-in"""
        if self.tracker.was_checked_in_before_shutdown:
            self.backend.handle_resume()
            self.backend.disableCheckinButton()
            self.backend.enable_buttons()
        else:
            self.backend.enableCheckinButton()
            self.backend.disable_buttons()
            self.is_checked_in = False
        
    def _save_connection_details(self):
        """Save the connection type details"""
        conn_type = self.check_connection()
        ip = self.get_public_ip()
        isp = self.get_isp_from_ip(ip)
        self.db_manager.get_connection_details(ip, conn_type, isp)
        
    def _start_location_tracking(self):
        """Save location tracking details"""
        self.location_source = QGeoPositionInfoSource.createDefaultSource(self.root)
        if self.location_source:
            self.location_source.positionUpdated.connect(self.on_position_updated)
            self.location_source.startUpdates()
        else:
            logging.info("No location source found")
        
    def _schedule_tasks(self):
        """Schedule tasks to be run on app load finish"""
        self.cleanup_old_data()
        
    def _validity_check(self):
        def gui_validity_check():
            valid_till_str = self.db_manager.get_valid_upto()
            if valid_till_str:
                try:
                    valid_dt = parse(valid_till_str).replace(tzinfo=None)
                    now_utc = datetime.now().replace(tzinfo=None, microsecond=0)
                    days_left = (valid_dt.date() - now_utc.date()).days
                    if days_left in [7, 5, 3, 2, 1]:
                        msg_box = QMessageBox(self.root)
                        msg_box.setText(f"Your organization validity will expire in {days_left} day{'s' if days_left > 1 else ''}. Please contact your admin to renew.")
                        msg_box.setWindowTitle("Validity Expiry Warning")
                        msg_box.setStandardButtons(QMessageBox.Ok)
                        msg_box.exec_()
                        msg_box.deleteLater()
                    if valid_dt <= now_utc:
                        self.show_notification("Validity Expired","Your organization validity has expired. You will be signed out.")
                        self.check_and_update_validity()
                    else:
                        time_diff = valid_dt - now_utc
                        MAX_TIMER_DELAY_MS = 14 * 24 * 60 * 60 * 1000  # 24 hours (adjust as needed)
                        delay_ms = max(0, min(int(time_diff.total_seconds() * 1000), MAX_TIMER_DELAY_MS))
                        if hasattr(self, 'validity_timer') and self.validity_timer:
                            self.validity_timer.stop()
                            self.validity_timer.deleteLater()
                        self.validity_timer = QTimer(self.root)
                        self.validity_timer.setSingleShot(True)
                        self.validity_timer.timeout.connect(self.check_and_update_validity)
                        self.validity_timer.start(delay_ms)
                        logging.info(f"Validity timer started for {delay_ms / 1000 / 60:.2f} minutes until {valid_dt}")
                except Exception as e:
                    logging.error(f"Error setting up validity timer: {e}")
                    self.auto_signout()
            else:
                logging.warning("No validity date found in DB")
                self.check_and_update_validity()
        QTimer.singleShot(0, gui_validity_check)

    def _on_sync_finished(self, result):
        """Handle sync worker completion on the main thread and update the UI safely."""
        try:
            if not result:
                logging.info("Sync finished with no data; skipping UI update")
                return
            logging.info("_on_sync_finished received data; injecting into JS")
            # Ensure inject_js_functions runs on the main thread
            try:
                self.inject_js_functions(result)
            except Exception as e:
                logging.error(f"Error injecting JS functions on main thread: {e}", exc_info=True)
        finally:
            gc.collect()

    def verify_organization(self, org_id):
        """Verify organization ID with server."""
        try:
            params = {"orgId": org_id, "productName": os.getenv("PRODUCT_NAME")}
            headers = {'accept': '*/*'}
            base_url = os.getenv("ORG_URL")
            url = f"{base_url}?{urllib.parse.urlencode(params)}"
            response = requests.get(url, headers=headers, timeout=10)
            if response.status_code in (200, 201):
                data = response.json()
                valid_till = parse(data["validTill"]).replace(tzinfo=pytz.utc)
                if valid_till > datetime.now(pytz.utc):
                    config_list = [{"key": item['configKey'], "value": item['configValue']} for item in data.get('orgConfigs', []) if 'configKey' in item and 'configValue' in item]
                    self.config_data = config_list
                    self.org_verified = True
                    self.org_dialog.accept()
                    self.db_manager.set_org(org_id)
                    self.db_manager.set_valid_upto(data["validTill"])
                    self.db_manager.enter_config(config_list)
                    self.create_email_screen(org_id)
                else:
                    try:
                        msg = QMessageBox(self.root)
                        msg.setIcon(QMessageBox.Warning)
                        msg.setWindowTitle("Access Expired")
                        msg.setText("Your account validity has expired.\nThe application will now close.")
                        msg.setStandardButtons(QMessageBox.Ok)
                        msg.setWindowFlags(msg.windowFlags() | Qt.WindowStaysOnTopHint)
                        msg.exec_()
                    except Exception:
                        QMessageBox.warning(None, "Access Expired", "Your account validity has expired.\nThe application will now close.")
                    self._cleanup_and_exit()
            else:
                raise ValueError(f"Invalid organization ID. Status code: {response.status_code}")
        except Exception as e:
            logging.error(f"Verification failed: {e}", exc_info=True)
            self.warning_label.setText("Invalid organization ID. Please try again.")
            self.reset_ui_after_failure()
        finally:
            gc.collect()
        
    def _show_spinner_and_authenticate(self, form_widgets):
        """Show spinner and start authentication."""
        self.spinner_container, self.spinner_movie = self.show_spinner_overlay(self.current_dialog, base_path)
        host_name = socket.gethostname()
        trial_url = self.db_manager.get_single_config("base_url")
        self.auth_thread = QThread()
        self._threads.append(self.auth_thread)
        self.auth_worker = AuthWorker(self.email, self.unique_key, self.password, self.org_id, host_name, trial_url)
        self.auth_worker.moveToThread(self.auth_thread)
        self.auth_worker.finished.connect(lambda result: self._handle_auth_result(result, form_widgets))
        self.auth_thread.started.connect(self.auth_worker.run)
        self.auth_thread.finished.connect(self.auth_thread.deleteLater)
        self.auth_thread.finished.connect(lambda: self._threads.remove(self.auth_thread))
        self.auth_thread.start()

    def show_spinner_overlay(self, dialog, base_path):
        spinner_overlay = QWidget(dialog)
        self._weak_refs['spinner_overlay'] = weakref.ref(spinner_overlay)
        spinner_overlay.setObjectName("spinnerOverlay")
        spinner_overlay.setStyleSheet("""
            #spinnerOverlay {
                background-color: rgba(0, 0, 0, 100);
                border: none;
            }""")
        spinner_overlay.setGeometry(dialog.rect())
        spinner_overlay.setAttribute(Qt.WA_StyledBackground, True)
        spinner_overlay.setWindowFlags(Qt.Widget | Qt.FramelessWindowHint)
        spinner_overlay.setContentsMargins(0, 0, 0, 0)
        spinner_layout = QVBoxLayout(spinner_overlay)
        spinner_layout.setContentsMargins(0, 0, 0, 0)
        spinner_layout.setSpacing(0)
        spinner_layout.setAlignment(Qt.AlignCenter)
        spinner_label = QLabel()
        spinner_label.setFixedSize(75, 75)
        spinner_label.setAlignment(Qt.AlignCenter)
        spinner_movie = QMovie(os.path.join(base_path, "images/setting.gif"))
        self._weak_refs['spinner_movie'] = weakref.ref(spinner_movie)
        spinner_movie.setScaledSize(QSize(75, 75))
        spinner_label.setMovie(spinner_movie)
        spinner_movie.start()
        auth_label = QLabel("Authenticating...")
        auth_label.setAlignment(Qt.AlignCenter)
        auth_label.setStyleSheet("font-size: 13px; color: #333; margin: 0px; padding: 0px;")
        spinner_layout.addWidget(spinner_label)
        spinner_layout.addWidget(auth_label)
        effect = QGraphicsOpacityEffect(spinner_overlay)
        spinner_overlay.setGraphicsEffect(effect)
        fade_anim = QPropertyAnimation(effect, b"opacity")
        fade_anim.setDuration(300)
        fade_anim.setStartValue(0)
        fade_anim.setEndValue(1)
        fade_anim.setEasingCurve(QEasingCurve.InOutQuad)
        fade_anim.start()
        self._animations.append(fade_anim)
        spinner_overlay.show()
        return spinner_overlay, spinner_movie
        
    def fade_widgets(self, widgets, start_opacity, end_opacity, duration=600, on_done=None):
        """Fade widgets with animation."""
        animations = []
        def on_finished():
            for w in widgets:
                if end_opacity == 0:
                    w.hide()
                w.setGraphicsEffect(None)
            if on_done:
                on_done()
            for anim in animations:
                if anim in self._animations:
                    anim.deleteLater()
                    self._animations.remove(anim)
            gc.collect()
        
        for widget in widgets:
            old_effect = widget.graphicsEffect()
            if old_effect:
                widget.setGraphicsEffect(None)
                old_effect.deleteLater()
            effect = QGraphicsOpacityEffect(widget)
            widget.setGraphicsEffect(effect)
            anim = QPropertyAnimation(effect, b"opacity")
            anim.setDuration(duration)
            anim.setStartValue(start_opacity)
            anim.setEndValue(end_opacity)
            anim.setEasingCurve(QEasingCurve.InOutQuad)
            anim.start()
            animations.append(anim)
            self._animations.append(anim)
        
        QTimer.singleShot(duration, on_finished)
        
    def compare_and_sync(self, server_data, client_data):
        """Compare server and client data for synchronization."""
        changes_detected = False
        for key, value in server_data.items():
            if key in client_data:
                if value != client_data[key]:
                    logging.info(f"Field '{key}' has changed. Server: {value}, Client: {client_data[key]}")
                    changes_detected = True
            else:
                logging.info(f"Field '{key}' is missing in client data.")
                changes_detected = True
        return changes_detected
        
    def get_connection_type(self):
        """Get network connection type."""
        interfaces = psutil.net_if_addrs()
        for interface in interfaces:
            if interface in psutil.net_if_stats() and psutil.net_if_stats()[interface].isup:
                if "mobile" in interface.lower() or "hotspot" in interface.lower():
                    return "Mobile Hotspot"
                elif "wifi" in interface.lower():
                    return "WiFi"
                elif "ethernet" in interface.lower() or "wired" in interface.lower():
                    return "Ethernet"
                elif "vpn" in interface.lower():
                    return "VPN"
                elif "bluetooth" in interface.lower():
                    return "Bluetooth"
                else:
                    return interface
        return None
        
    def get_public_ip(self):
        """Get public IP address."""
        try:
            return requests.get('https://api.ipify.org').text
        except:
            return "Unable to get public IP"
    
    @log_function_call
    def inject_js_functions(self, server_data):
        """Inject JavaScript functions to update UI."""
        self.browser.page().runJavaScript("""
            if (typeof updateUserProfile !== 'function') {
                window.updateUserProfile = function(user_details) {
                    try {
                        const user_data = typeof user_details === 'string'
                            ? JSON.parse(user_details)
                            : user_details;
                        const nameElement = document.getElementById('profile-name');
                        // index.html uses 'profile-mail' for the email span
                        const emailElement = document.getElementById('profile-mail') || document.getElementById('profile-email');
                        const avatarElement = document.getElementById('profile-avatar');
                        const roleElement = document.getElementById('profile-role');
                        const contactElement = document.getElementById('profile-contact');
                        const connectivityElement = document.getElementById('connType');
                        const locationElement = document.getElementById('location');
    
                        if (nameElement) {
                            const first = user_data.first_name || user_data.firstName || user_data.first || '';
                            const last = user_data.last_name || user_data.lastName || user_data.last || '';
                            nameElement.innerText = (first + ' ' + last).trim() || user_data.email || 'Unknown User';
                        }
                        if (emailElement) {
                            emailElement.innerText = user_data.email || user_data.emailAddress || user_data.mail || '';
                        }
                        if (avatarElement) {
                            const firstInitial = (user_data.first_name || user_data.firstName || '').charAt(0).toUpperCase() || '';
                            const lastInitial = (user_data.last_name || user_data.lastName || '').charAt(0).toUpperCase() || '';
                            avatarElement.innerText = (firstInitial + lastInitial) || 'U';
                        }
                        // Update all elements that might share the same id (some templates duplicate ids)
                        const roleElements = document.querySelectorAll('#profile-role');
                        if (roleElements && roleElements.length) {
                            roleElements.forEach(function(el) {
                                el.innerText = user_data.designation || user_data.role || 'Employee';
                            });
                        }
                        // Update email in both possible ids
                        const emailElements = document.querySelectorAll('#profile-mail, #profile-email');
                        if (emailElements && emailElements.length) {
                            emailElements.forEach(function(el) {
                                el.innerText = user_data.email || user_data.emailAddress || user_data.mail || '';
                            });
                        }
                        if (contactElement) {
                            contactElement.innerText = user_data.phone_number || user_data.phone || user_data.contact || '';
                        }
                        if (connectivityElement) {
                            connectivityElement.innerText = user_data.connection_type || user_data.connType || 'Unknown';
                        }
                         if (locationElement) {
                            locationElement.innerText = user_data.location || 'Unknown';}                 
                    } catch (e) {
                        console.error("Input data:", user_details);
                        console.error("Error updating user profile:", e);
                        try {
                            if (document.getElementById('profile-name')) document.getElementById('profile-name').innerText = "Unknown User";
                            if (document.getElementById('profile-mail')) document.getElementById('profile-mail').innerText = "unknown@email.com";
                            if (document.getElementById('profile-avatar')) document.getElementById('profile-avatar').innerText = "U";
                        } catch (ie) { /* ignore */ }
                    }
                };
            }""")
        user_json = json.dumps(server_data)
        self.browser.page().runJavaScript(f"updateUserProfile({user_json})")
        gc.collect()
        
    def check_employee_status(self):
        """Check employee status."""
        try:
            status_url = os.getenv('STATUS_URL')
            params = {'userId': self.db_manager.get_userid(), 'productName': os.getenv('PRODUCT_NAME')}
            response = requests.get(url=status_url, params=params, timeout=10)
            result = response.json()
            if result.get('isActive') == False:
                QMessageBox.critical(self.root, "Employee Status", f"Your employee status is inactive. Please contact your administrator.")
                self.auto_signout()
                self.root.destroy()
        except Exception as e:
            logging.error(f"Error checking employee status: {e}", exc_info=True)
            QMessageBox.critical(self.root, "Error", f"Error checking employee status: {e}")
            self.auto_signout()
        finally:
            gc.collect()
        
    def check_connection(self):
        """Check network connection type."""
        return self.get_connection_type()
    
    def get_isp_from_ip(self, ip):
        """Get ISP from IP address."""
        try:
            response = requests.get(f"https://ipinfo.io/{ip}/json")
            result = response.json()
            return result.get("org", "ISP not found")
        except Exception as e:
            return f"Error: {e}"
        
    def on_position_updated(self, position):
        """Handle position updates."""
        coordinate = position.coordinate()
        latitude = coordinate.latitude()
        longitude = coordinate.longitude()
        self.update_html_with_coordinates(latitude, longitude)
        if self.location_source:
            self.location_source.stopUpdates()
            self.location_source.deleteLater()
            self.location_source = None
        gc.collect()
        
    def cleanup_old_data(self):
        """Cleanup old data and logs."""
        seven_days = datetime.now().replace(tzinfo=None, microsecond=0) - timedelta(days=2)
        self.db_manager.delete_old_data(seven_days)
        self.clean_up_old_logs(seven_days)
        gc.collect()
        
    def check_and_update_validity(self):
        """Check and update organization validity by hitting the API, update DB if valid, else auto-signout."""
        try:
            org_id = self.db_manager.get_org_id()
            if not org_id:
                logging.error("No organization ID found for validity check")
                self.show_expiration_popup()
                self.auto_signout()
                return
            params = {"orgId": org_id, "productName": os.getenv("PRODUCT_NAME")}
            headers = {"accept": "*/*"}
            response = requests.get(
                url=os.getenv("ORG_URL"),
                params=params,
                headers=headers,
                timeout=10)
            response.raise_for_status()  # Raise exception for non-200 status codes
            data = response.json()
            valid_till = parse(data.get("validTill")).replace(tzinfo=pytz.utc)
            now_utc = datetime.now(pytz.utc)
            if valid_till >= now_utc:
                config_list = [
                    {"key": item['configKey'], "value": item['configValue']}
                    for item in data.get('orgConfigs', [])
                    if 'configKey' in item and 'configValue' in item]
                self.db_manager.set_valid_upto(data["validTill"])
                if config_list:
                    self.db_manager.update_config(config_list)
                    self.tracker.monitoring_updates()
                    logging.info(f"Validity updated in DB to {data['validTill']}")
                delay_ms = int((valid_till - now_utc).total_seconds() * 1000)
                if hasattr(self, 'validity_timer') and self.validity_timer:
                    self.validity_timer.stop()
                    self.validity_timer.deleteLater()
                self.validity_timer = QTimer(self.root)
                self.validity_timer.setSingleShot(True)
                self.validity_timer.timeout.connect(self.check_and_update_validity)
                self.validity_timer.start(delay_ms)
                logging.info(f"New validity timer started for {delay_ms / 1000 / 60:.2f} minutes until {valid_till}")
            else:
                # Validity expired, trigger auto-signout
                logging.warning("Organization validity expired")
                self.show_expiration_popup()
                self.auto_signout()
        except Exception as e:
            logging.error(f"Error in check_and_update_validity: {e}", exc_info=True)
            self.show_expiration_popup()
            self.auto_signout()
        finally:
            gc.collect()
        
    def reset_ui_after_failure(self) -> None:
        """Reset UI after verification failure."""
        if hasattr(self, 'spinner_movie') and self.spinner_movie:
            self.spinner_movie.stop()
        if hasattr(self, 'spinner_container') and self.spinner_container:
            self.spinner_container.hide()
        if hasattr(self, 'org_input') and self.org_input:
            self.org_input.setEnabled(True)
        if hasattr(self, 'submit_btn') and self.submit_btn:
            self.submit_btn.setEnabled(True)
        QApplication.processEvents()
        gc.collect()
        
    def _handle_auth_result(self, result, form_widgets):
        """Process authentication result."""
        self.hide_spinner(self.spinner_container, self.spinner_movie)
        if result.get('data').get('code') != 200:
            self._handle_auth_failure(result, form_widgets)
            return
        user_id = result.get('data').get('data').get('userId')
        auth_data = result["data"]
        self.db_manager.set_user_id(user_id)     
        self._verify_user_in_system(
            email=self.email,
            org_id=self.org_id,
            auth_data=auth_data,
            form_widgets=form_widgets)

    def hide_spinner(self, spinner_container, spinner_movie) -> None:
        """Fade out and hide spinner overlay."""
        if not spinner_container or not spinner_movie:
            return
        
        def finish():
            spinner_movie.stop()
            spinner_container.hide()
            spinner_container.setGraphicsEffect(None)
            spinner_container.deleteLater()
            self._weak_refs.pop('spinner_container', None)
            self._weak_refs.pop('spinner_movie', None)
            gc.collect()
        
        effect = QGraphicsOpacityEffect(spinner_container)
        spinner_container.setGraphicsEffect(effect)
        anim = QPropertyAnimation(effect, b"opacity")
        anim.setDuration(400)
        anim.setStartValue(1)
        anim.setEndValue(0)
        anim.setEasingCurve(QEasingCurve.InOutQuad)
        anim.finished.connect(finish)
        anim.start()
        self._animations.append(anim)
        
    def update_html_with_coordinates(self, lat, lon):
        """Update HTML with geolocation data."""
        url = f"https://api.opencagedata.com/geocode/v1/json?q={lat}+{lon}&key=e992d8b4f80e45d1be5045a5f27b0a79"
        response = requests.get(url)
        if response.status_code in (200, 201):
            data = response.json()
            address = data["results"][0]["formatted"]
            self.backend.location = address
            self.db_manager.save_location_details(address, lat, lon)
        gc.collect()
        
    def show_expiration_popup(self) -> None:
        """Show subscription expiration popup."""
        self.popup = QMessageBox(self.root)
        self._weak_refs['popup'] = weakref.ref(self.popup)
        self.popup.setWindowTitle("Validity Expired")
        self.popup.setText("You will be signed out now due to validity end. Please contact your admin to continue.")
        self.popup.setIcon(QMessageBox.Warning)
        self.popup.setStandardButtons(QMessageBox.Ok)
        self.popup.setWindowFlags(Qt.WindowStaysOnTopHint)
        self.popup.buttonClicked.connect(self.handle_popup_action)
        self.popup.exec_()
        self.popup.deleteLater()
        self._weak_refs.pop('popup', None)
        gc.collect()
        
    def _verify_user_in_system(self, email, org_id, auth_data, form_widgets):
        """Verify user in system database."""
        self.verification_thread = QThread()
        self._threads.append(self.verification_thread)
        with open(self.unique_file) as f:
            license_key = f.read()
        self.verification_worker = Worker(self._perform_db_verification,license_key,email, org_id)
        self.verification_worker.moveToThread(self.verification_thread)
        self.verification_worker.finished.connect(lambda result: self._on_verification_complete(result, auth_data, form_widgets))
        self.verification_worker.error.connect(lambda error: self._on_verification_error(error, form_widgets))
        self.verification_thread.started.connect(self.verification_worker.run)
        self.verification_thread.finished.connect(self._cleanup_verification)
        self.verification_thread.start()
        
    def handle_popup_action(self, button):
        """Handle popup button click."""
        try:
            self.auto_signout()
            logging.info("User signed out due to subscription expiration")
        except Exception as e:
            logging.error(f"Signout failed: {e}", exc_info=True)
        finally:
            gc.collect()
        
    def _perform_db_verification(self, license_key, email, org_id):
        """Perform database verification."""
        try:
            base_url = self.db_manager.get_single_config("base_url")
            if org_id is None:
                org_id = self.db_manager.get_org_id()
            user_id = self.db_manager.get_userid()
            self.api_cli = ProductivityAPIClient(self.db_manager.get_single_config("base_url"), self.db_manager.get_userid(), self.db_manager.get_org_id())
            response = self.api_cli.user_check(license_key, email, org_id, user_id)
            return {"exists": response.get("exists", False)}
        except Exception as e:
            endpoint = f"{base_url}/user_check"
            payload = {"email": email, "org_id": org_id}
            response = requests.post(endpoint, json=payload, timeout=10)
            if response.status_code == 200:
                return {"exists": response.json().get("exists", False)}
            raise Exception(f"Server error: {response.status_code}")
        
    def _on_verification_complete(self, result, auth_data, form_widgets):
        """Handle verification completion."""
        self._cleanup_verification()
        if result.get('exists'):
            self.org_id = self.db_manager.get_org_id()
            self.auth_data = auth_data
            self.check_device_registration(auth_data, self.org_id, self.current_dialog, form_widgets)
        else:
            self._show_access_denied(form_widgets, "License not purchased. Please contact your system administrator.")
            self._cleanup_and_exit()
        if hasattr(self, 'loader') and self.loader:
            self.loader.hide()
            self.loader.cleanup()
            self.loader.deleteLater()
            self.loader = None

    def _on_verification_error(self, error, form_widgets):
        """Handle verification errors."""
        self._cleanup_verification()
        self._show_access_denied(form_widgets, f"System verification failed: {error}\n\nPlease contact support.")
        if hasattr(self, 'loader') and self.loader:
            self.loader.hide()
            self.loader.cleanup()
            self.loader.deleteLater()
            self.loader = None

    def _cleanup_verification(self):
        """Clean up verification thread resources."""
        if hasattr(self, 'verification_thread'):
            self.verification_thread.quit()
            self.verification_thread.wait(500)
            if self.verification_thread in self._threads:
                self._threads.remove(self.verification_thread)
            self.verification_thread.deleteLater()
            del self.verification_thread
        if hasattr(self, 'verification_worker'):
            self.verification_worker.deleteLater()
            del self.verification_worker
        gc.collect()
        
    def check_device_registration(self, auth_data, org_id, dialog, form_widgets):
        """Check and handle device registration."""
        hostname = socket.gethostname()
        os_name = platform.system()
        device_info = {"device_name": hostname, "os_name": os_name}
        TRIAL_URL = self.db_manager.get_single_config("base_url")
        
        def handle_device_result(result):
            hostname = socket.gethostname()
            if result.get("success"):
                device_data = result["data"]
                matching_device = next((d for d in device_data['device_info'] if d.get('device_name') == hostname), None)
                if matching_device and matching_device.get('is_primary'):
                    self.set_primary_device(hostname, auth_data, org_id, dialog)
                else:
                    self.fade_widgets(form_widgets, 0, 1, 600)
            else:
                logging.error(f"Device registration failed: {result.get('error')}")
                QMessageBox.warning(dialog, "Device Registration Error", "Failed to register device. Please try again.")
                self.fade_widgets(form_widgets, 0, 1, 600)
            gc.collect()
        user_id = self.db_manager.get_userid()
        self.device_thread = QThread()
        self._threads.append(self.device_thread)
        self.device_worker = DeviceWorker(self.email, org_id, device_info, TRIAL_URL, user_id, self.api_cli)
        self.device_worker.moveToThread(self.device_thread)
        self.device_worker.finished.connect(handle_device_result)
        self.device_thread.started.connect(self.device_worker.run)
        self.device_worker.finished.connect(self.device_thread.quit)
        self.device_worker.finished.connect(self.device_worker.deleteLater)
        self.device_thread.finished.connect(self.device_thread.deleteLater)
        self.device_thread.finished.connect(lambda: self._threads.remove(self.device_thread))
        self.device_thread.start()

        
    def _show_access_denied(self, form_widgets, message):
        """Show access denied message."""
        self._restore_ui(form_widgets)
        msg = QMessageBox(self.current_dialog)
        msg.setIcon(QMessageBox.Critical)
        msg.setWindowTitle("Access Denied")
        msg.setText(message)
        msg.setStandardButtons(QMessageBox.Ok)
        msg.buttonClicked.connect(self._cleanup_and_exit)
        msg.exec_()
        msg.deleteLater()
        gc.collect()
        
    def set_primary_device(self, selected_device_id: str, auth_data: Dict, org_id: str, dialog: QDialog) -> None:
        """Set the selected device as primary."""
        TRIAL_URL = self.db_manager.get_single_config("base_url")
        
        def handle_primary_result(result: Dict) -> None:
            if result.get("success"):
                self.complete_login(auth_data, org_id, dialog)
            else:
                logging.error(f"Failed to set primary device: {result.get('error')}")
                QMessageBox.warning(dialog, "Device Setup Error", "Failed to set primary device. Please try again.")
            gc.collect()
        
        user_id = self.db_manager.get_userid()
        self.primary_thread = QThread()
        self._threads.append(self.primary_thread)
        self.primary_worker = PrimaryDeviceWorker(org_id, self.email, self.unique_key, selected_device_id, TRIAL_URL, user_id, self.api_cli)
        self.primary_worker.moveToThread(self.primary_thread)
        self.primary_worker.finished.connect(handle_primary_result)
        self.primary_thread.started.connect(self.primary_worker.run)
        self.primary_worker.finished.connect(self.primary_thread.quit)
        self.primary_worker.finished.connect(self.primary_worker.deleteLater)
        self.primary_thread.finished.connect(self.primary_thread.deleteLater)
        self.primary_thread.finished.connect(lambda: self._threads.remove(self.primary_thread))
        self.primary_thread.start()

    def auto_checkout_inactive(self):
        msg = QMessageBox(self.root)
        msg.setIcon(QMessageBox.Information)
        msg.setWindowTitle("Checkout Reminder")
        msg.setText("You will be checked out now.")
        msg.setStandardButtons(QMessageBox.Ok)
        msg.exec_()

    def complete_login(self, auth_data, org_id, dialog):
        """Complete the login process."""
        data = auth_data
        if data.get('status') == 'success':
            QMessageBox.information(dialog, "Authentication Successful", f"Welcome to EPIC! The Desktop Agent is now running. All work activity during business hours is monitored for productivity reporting. Your data is secure and stays within your organization.!")
            logging.info("Creating main UI after successful authentication")
            self.user_data = self.db_manager.fetch_and_log_user_data(data, org_id)
            self.db_manager.set_email(self.email, self.unique_key)
            dialog.accept()
            dialog.deleteLater()
            self.create_main_ui()
        else:
            QMessageBox.information(dialog, "Authentication Failed", "Login failed. Please check your credentials.")
            logging.error("Failed to authenticate user")
            self._cleanup_and_exit()
        gc.collect()

    class CheckoutWorker(QThread):
        """Worker thread for checkout operations."""
        checkout_success = pyqtSignal()
        checkout_error = pyqtSignal(str)

        def __init__(self, tracker, checkout_time: datetime):
            super().__init__()
            self.tracker = tracker
            self.checkout_time = checkout_time

        def run(self) -> None:
            try:
                self.tracker.checkout(self.checkout_time)
                self.checkout_success.emit()
            except Exception as e:
                self.checkout_error.emit(str(e))
            finally:
                gc.collect()

    def perform_checkout(self) -> bool:
        """Perform checkout operation."""
        try:
            self._checkout_finalized = False
            if self.checkout_event:
                logging.info("Triggering checkout_event from GUI")
                self.checkout_event.set()
            
            self.loader_label = LoaderLabel(self.root)
            self._weak_refs['loader_label'] = weakref.ref(self.loader_label)
            self.loader_label.show()
            
            def hide_and_destroy():
                if self._checkout_finalized:
                    return
                self._checkout_finalized = True
                if self.loader_label and self.loader_label.isVisible():
                    self.loader_label.hide()
                    self.loader_label.deleteLater()
                    self._weak_refs.pop('loader_label', None)
                self.root.destroy()
                QMessageBox.information(None, "Check-Out", "You have successfully checked out.")
                self._cleanup_and_exit()
            
            QTimer.singleShot(5000, hide_and_destroy)
            utc_now = datetime.now(pytz.utc)
            checkout_time = utc_now
            
            self.checkout_thread = self.CheckoutWorker(self.tracker, checkout_time)
            self._threads.append(self.checkout_thread)
            self.checkout_thread.checkout_success.connect(self._checkout_success)
            self.checkout_thread.checkout_error.connect(self._checkout_error)
            self.checkout_thread.start()
            return True
        except Exception as e:
            logging.info(e)
            #self.backend.log_activity(f"Exception during checkout: {str(e)}", ist_now)
            QMessageBox.critical(self.root, "Check-Out Error", "An unexpected error occurred during checkout.")
            return False
        finally:
            gc.collect()

    def _checkout_success(self) -> None:
        """Handle successful checkout."""
        if hasattr(self, "_checkout_finalized") and self._checkout_finalized:
            return
        self._checkout_finalized = True
        if self.loader_label and self.loader_label.isVisible():
            self.loader_label.hide()
            self.loader_label.deleteLater()
            self._weak_refs.pop('loader_label', None)
        if hasattr(self, "checkout_thread") and self.checkout_thread:
            self.checkout_thread.quit()
            self.checkout_thread.wait()
            self.checkout_thread.deleteLater()
            if self.checkout_thread in self._threads:
                self._threads.remove(self.checkout_thread)
            self.checkout_thread = None
        self.root.destroy()
        QMessageBox.information(self.root, "Check-Out", "You have successfully checked out.")
        self._cleanup_and_exit()
        gc.collect()

    def _checkout_error(self, error_message):
        logging.error(f"Checkout error: {error_message}")
        QMessageBox.critical(
            self.root,
            "Check-Out Error",
            "An unexpected error occurred during checkout.",
        )


    def _handle_auth_failure(self, error_data: Dict, form_widgets: List[QWidget]) -> None:
        """Handle authentication failure."""
        error_msg = error_data.get('message', 'Authentication failed')
        logging.error(f"Authentication failed: {error_msg}")
        self._restore_ui(form_widgets)
        QMessageBox.warning(self.current_dialog, "Login Error", error_msg)